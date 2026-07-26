//! Integration tests: lock in the observable behavior of the burxt compiler.
//!
//! Data-driven layout:
//!   tests/pass/NAME.bx  + NAME.stdout  — must compile & run; stdout must match exactly.
//!   tests/fail/NAME.bx  + NAME.stderr  — must be rejected; stderr must contain the text.
//!   tests/panic/NAME.bx + NAME.stderr  — must compile, but die at runtime with
//!                                        a nonzero exit and that text on stderr.
//!
//! Each program is compiled with the real `burxt` binary (CARGO_BIN_EXE_burxt)
//! inside a scratch directory, so executables and object files never land in
//! the repository. Adding a test = dropping two files in the right directory.
//!
//! Any non-.bx, non-expectation file in a test directory is a *fixture*: it is
//! copied into the scratch directory before the programs run, so a program that
//! reads a file has something to read.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Collect (program, expected-text) pairs from tests/<dir>, where the expected
/// text lives in a sibling file with the given extension.
fn cases(dir: &str, expected_ext: &str) -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(dir);
    let mut out = Vec::new();
    for entry in fs::read_dir(&root).unwrap_or_else(|e| panic!("cannot read {}: {}", root.display(), e)) {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("bx") {
            let expected_path = path.with_extension(expected_ext);
            let expected = fs::read_to_string(&expected_path)
                .unwrap_or_else(|e| panic!("missing expectation file {}: {}", expected_path.display(), e));
            out.push((path, expected));
        }
    }
    out.sort();
    assert!(!out.is_empty(), "no .bx programs found in tests/{}", dir);
    out
}

/// Run `burxt <cmd> <program>` in a scratch working directory.
fn burxt(cmd: &str, program: &Path, workdir: &Path) -> Output {
    fs::create_dir_all(workdir).unwrap();
    Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg(cmd)
        .arg(program)
        .current_dir(workdir)
        .output()
        .expect("failed to spawn burxt")
}

/// Copy tests/<dir>'s fixture files (anything that is not a program or an
/// expectation) into the scratch directory the programs run in.
fn install_fixtures(dir: &str, workdir: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(dir);
    fs::create_dir_all(workdir).unwrap();
    for entry in fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "bx" | "stdout" | "stderr") {
            fs::copy(&path, workdir.join(path.file_name().unwrap())).unwrap();
        }
    }
}

fn scratch_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("burxt-tests-{}-{}", std::process::id(), tag))
}

#[test]
fn pass_programs_produce_expected_stdout() {
    let scratch = scratch_dir("pass");
    install_fixtures("pass", &scratch);
    let mut failures = Vec::new();
    for (program, expected) in cases("pass", "stdout") {
        let out = burxt("run", &program, &scratch);
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            failures.push(format!(
                "{}: expected success, but compilation/run failed:\n{}",
                program.display(),
                String::from_utf8_lossy(&out.stderr)
            ));
        } else if stdout != expected {
            failures.push(format!(
                "{}: stdout mismatch\n  expected: {:?}\n  actual:   {:?}",
                program.display(),
                expected,
                stdout
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn panic_programs_die_cleanly_at_runtime() {
    let scratch = scratch_dir("panic");
    install_fixtures("panic", &scratch);
    let mut failures = Vec::new();
    for (program, expected) in cases("panic", "stderr") {
        let needle = expected.trim();
        let out = burxt("run", &program, &scratch);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            failures.push(format!(
                "{}: expected a runtime error, but it ran successfully",
                program.display()
            ));
        } else if !stderr.contains(needle) {
            failures.push(format!(
                "{}: wrong runtime error\n  expected to contain: {:?}\n  actual stderr:       {:?}",
                program.display(),
                needle,
                stderr
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The forward guarantee the object model depends on: an aggregate's layout is
/// EXACTLY its declared fields, in order, standard alignment — no type tag, no
/// vtable pointer, no refcount, no hidden header word. If this ever fails,
/// adding a trait implementation could move a field, and codegen written
/// against these offsets would break.
#[test]
fn struct_layout_has_no_hidden_header() {
    let scratch = scratch_dir("layout");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("layout_probe.bx");
    fs::write(
        &program,
        "struct Money { amount: Decimal<2> }\n\
         struct LineItem { price: Decimal<2>, qty: Int }\n\
         struct Order { total: Money, items: Int, label: String }\n\
         print(1);\n",
    )
    .unwrap();

    let out = burxt("layout", &program, &scratch);
    let report = String::from_utf8_lossy(&out.stdout);
    let expected = "\
Money: size 8 align 8
  +0 Decimal<2> (8 bytes)
LineItem: size 16 align 8
  +0 Decimal<2> (8 bytes)
  +8 Int (8 bytes)
Order: size 24 align 8
  +0 Money (8 bytes)
  +8 Int (8 bytes)
  +16 String (8 bytes)
";
    let _ = fs::remove_dir_all(&scratch);
    assert!(out.status.success(), "layout command failed: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        report, expected,
        "layout drifted — a hidden header or reordering would break the object model"
    );
}

/// The A4.5 layout guarantee, cashed in by A4.6: a struct's field offsets must
/// be byte-identical whether or not it is ever used as a trait object, because
/// the vtable lives OUTSIDE the value. Also checks the pay-for-what-you-use
/// rule: a program with no `dyn` emits no vtable at all.
#[test]
fn dyn_does_not_change_layout_and_costs_nothing_unused() {
    let scratch = scratch_dir("dyn-layout");
    fs::create_dir_all(&scratch).unwrap();

    let common = "trait Priced { fn price(self) -> Decimal<2> }\n\
                  struct Book { cost: Decimal<2>, pages: Int }\n\
                  impl Priced for Book {\n\
                  fn (self: Book) price() -> Decimal<2> { return self.cost; }\n\
                  }\n\
                  let b: Book = Book { cost: 1.00, pages: 2 };\n";

    let static_only = scratch.join("static_only.bx");
    fs::write(&static_only, format!("{}print(b.price());\n", common)).unwrap();

    let with_dyn = scratch.join("with_dyn.bx");
    fs::write(
        &with_dyn,
        format!("{}let d: dyn Priced = b;\nprint(d.price());\n", common),
    )
    .unwrap();

    let layout_static = burxt("layout", &static_only, &scratch);
    let layout_dyn = burxt("layout", &with_dyn, &scratch);
    let ir_static = burxt("emit-ir", &static_only, &scratch);
    let ir_dyn = burxt("emit-ir", &with_dyn, &scratch);

    let l_static = String::from_utf8_lossy(&layout_static.stdout).to_string();
    let l_dyn = String::from_utf8_lossy(&layout_dyn.stdout).to_string();
    let s_ir = String::from_utf8_lossy(&ir_static.stdout).to_string();
    let d_ir = String::from_utf8_lossy(&ir_dyn.stdout).to_string();
    let _ = fs::remove_dir_all(&scratch);

    assert!(
        l_static.contains("+0 Decimal<2>") && l_static.contains("+8 Int"),
        "unexpected baseline layout:\n{}",
        l_static
    );
    assert_eq!(
        l_static, l_dyn,
        "becoming a trait object moved a field — the vtable must live outside the value"
    );
    assert!(
        !s_ir.contains("bx.vtable"),
        "a program with no `dyn` must emit no vtable"
    );
    assert!(
        d_ir.contains("bx.vtable.Priced.Book"),
        "a `dyn` program must emit the (Type, Trait) vtable"
    );
}

#[test]
fn fail_programs_are_rejected_with_expected_error() {
    let scratch = scratch_dir("fail");
    let mut failures = Vec::new();
    for (program, expected) in cases("fail", "stderr") {
        let needle = expected.trim();
        let out = burxt("build", &program, &scratch);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            failures.push(format!(
                "{}: expected rejection, but it compiled successfully",
                program.display()
            ));
        } else if !stderr.contains(needle) {
            failures.push(format!(
                "{}: wrong error\n  expected to contain: {:?}\n  actual stderr:       {:?}",
                program.display(),
                needle,
                stderr
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// `return tail` is a GUARANTEE, so it must be visible in the IR as `musttail`
/// — the marker LLVM refuses to compile unless the call really is a tail call.
/// A plain `tail` marker, or none, would make the promise a hope. The companion
/// pass test proves the behavior (50M frames); this proves the mechanism, so a
/// future refactor cannot quietly downgrade it.
#[test]
fn tail_calls_are_emitted_as_musttail() {
    let scratch = scratch_dir("musttail");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("tail_probe.bx");
    fs::write(
        &program,
        "fn down(n: Int, acc: Int) -> Int {\n\
         if n <= 0 { return acc; }\n\
         return tail down(n - 1, acc + 1);\n\
         }\n\
         fn plain(n: Int, acc: Int) -> Int {\n\
         if n <= 0 { return acc; }\n\
         return plain(n - 1, acc + 1);\n\
         }\n\
         print(down(3, 0) + plain(3, 0));\n",
    )
    .unwrap();

    let out = burxt("emit-ir", &program, &scratch);
    let ir = String::from_utf8_lossy(&out.stdout);
    let _ = fs::remove_dir_all(&scratch);
    assert!(
        out.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let musttail: Vec<&str> = ir.lines().filter(|l| l.contains("musttail call")).collect();
    assert_eq!(
        musttail.len(),
        1,
        "expected exactly one `musttail call` (the `return tail` site), found {:?}",
        musttail
    );
    assert!(
        musttail[0].contains("@bx.down"),
        "the musttail call should be the one written with `tail`: {}",
        musttail[0]
    );
    // And the guarantee must NOT be applied to a call nobody asked about.
    assert!(
        !ir.contains("musttail call i64 @bx.plain"),
        "an ordinary recursive call was marked musttail — the guarantee must be \
         explicit, never inferred"
    );
}

/// NOVELTY §1, the guarantee that has to be checked against a REAL C boundary
/// rather than described: a `Decimal<2> as scaled` arrives as the exact scaled
/// integer, an `Int` crossing as `CDouble` arrives as the same number, and an
/// `Int` too large to be a double exactly dies with a named error instead of
/// quietly becoming its neighbour.
///
/// This also exercises linker pass-through — an `extern fn` is only half an
/// FFI; the other half is a real object to link against.
#[test]
fn money_and_integers_cross_into_c_exactly() {
    let scratch = scratch_dir("boundary");
    fs::create_dir_all(&scratch).unwrap();

    fs::write(
        scratch.join("cside.c"),
        "#include <stdio.h>\n\
         long long record_cents(long long scaled) { printf(\"%lld\\n\", scaled); return scaled; }\n\
         long long take_double(double d) { printf(\"%.0f\\n\", d); return (long long)d; }\n",
    )
    .unwrap();
    let cc = Command::new("cc")
        .args(["-c", "cside.c", "-o", "cside.o"])
        .current_dir(&scratch)
        .status()
        .expect("failed to invoke cc");
    assert!(cc.success(), "could not build the C side of the boundary test");

    let program = scratch.join("boundary.bx");
    fs::write(
        &program,
        "extern fn record_cents(amount: Decimal<2> as scaled) -> Int;\n\
         extern fn take_double(n: CDouble) -> Int;\n\
         let price: Decimal<2> = $19.99;\n\
         print(record_cents(price));\n\
         print(take_double(9007199254740992));\n",
    )
    .unwrap();

    let run = |src: &str, args: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("run")
            .arg(src)
            .args(args)
            .current_dir(&scratch)
            .output()
            .expect("failed to spawn burxt")
    };

    let out = run("boundary.bx", &["cside.o"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "boundary program failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // C printed the value it received, then Burxt printed what came back. The
    // scaled integer must be exactly 1999 — not 1998, not 2000, not 19.99.
    assert_eq!(
        stdout, "1999\n1999\n9007199254740992\n9007199254740992\n",
        "a value changed while crossing into C"
    );

    // 2^53 + 1 is not representable as a double, so the crossing must be a
    // named error and exit 70 — never a silently different integer.
    fs::write(
        scratch.join("over.bx"),
        "extern fn take_double(n: CDouble) -> Int;\n\
         print(take_double(9007199254740993));\n",
    )
    .unwrap();
    let over = run("over.bx", &["cside.o"]);
    let stderr = String::from_utf8_lossy(&over.stderr);
    let code = over.status.code();
    let _ = fs::remove_dir_all(&scratch);
    assert_eq!(code, Some(70), "an inexact crossing must exit 70, got {:?}", code);
    assert!(
        stderr.contains("cannot cross as a C double exactly"),
        "the failure must name itself, got: {}",
        stderr
    );
}

/// The editor grammar and the compiler must not drift. A keyword the lexer knows
/// but the grammar does not is a word that compiles and is not highlighted —
/// which is how a language starts feeling unfinished. The grammar is also the
/// artifact GitHub's Linguist consumes, so this keeps that submission honest too.
///
/// Deliberately dependency-free: it reads the compiler's own keyword table out of
/// the source rather than duplicating the list here, because a duplicated list is
/// the thing that drifts.
#[test]
fn editor_grammar_knows_every_keyword_the_compiler_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lexer = fs::read_to_string(root.join("src/lexer.rs")).unwrap();
    let typeck = fs::read_to_string(root.join("src/typeck.rs")).unwrap();
    let grammar =
        fs::read_to_string(root.join("editors/vscode/syntaxes/burxt.tmLanguage.json")).unwrap();

    // Keywords, from the lexer's `"word" => Token::Variant` table.
    let mut words: Vec<String> = lexer
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix('"')?;
            let (word, tail) = rest.split_once('"')?;
            tail.trim_start().starts_with("=> Token::").then(|| word.to_string())
        })
        .collect();
    assert!(
        words.len() > 20,
        "failed to read the keyword table out of src/lexer.rs (found {:?})",
        words
    );

    // Built-in functions, from the reserved-name check in the typechecker.
    for chunk in typeck.split("f.name == \"").skip(1) {
        if let Some((name, _)) = chunk.split_once('"') {
            words.push(name.to_string());
        }
    }

    // Search only the grammar's PATTERNS, never its prose: a keyword mentioned in
    // a comment is not a keyword that highlights. (Verified by mutation — the
    // looser "anywhere in the file" version passed after the `tail` rule was
    // deleted, because the word survived in a comment.)
    let patterns: String = grammar
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("\"match\"") || t.starts_with("\"begin\"") || t.starts_with("\"end\"")
        })
        .collect::<Vec<_>>()
        .join("\n")
        // `\b` is a word-boundary assertion, and its own `b` would otherwise
        // look like a letter attached to the keyword it precedes.
        .replace("\\\\b", " ");
    assert!(
        patterns.len() > 500,
        "failed to read the grammar's patterns (got {} bytes)",
        patterns.len()
    );

    let known_word = |w: &str| {
        // A word, not a substring: `as` must not be satisfied by `class`.
        patterns.match_indices(w).any(|(i, _)| {
            let before = patterns[..i].chars().next_back();
            let after = patterns[i + w.len()..].chars().next();
            let boundary = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            boundary(before) && boundary(after)
        })
    };

    let missing: Vec<&String> = words.iter().filter(|w| !known_word(w)).collect();
    assert!(
        missing.is_empty(),
        "these words are known to the compiler but absent from the editor grammar: {:?}\n\
         Add them to editors/vscode/syntaxes/burxt.tmLanguage.json",
        missing
    );
}

/// Every program in `examples/` must still typecheck. Examples are the first
/// thing a newcomer reads, and a broken one is worse than none — but nothing was
/// checking them, so they could rot silently while the suite stayed green.
///
/// `check` is used rather than `run` on purpose: it needs no working directory,
/// no linker, and no LLVM, so this stays fast enough to never be skipped.
/// Files under `examples/inputs/` are DATA for other examples to read, not
/// programs, so they are not checked — a directory rather than an exception list,
/// because exception lists rot.
#[test]
fn every_example_still_typechecks() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let scratch = scratch_dir("examples");
    fs::create_dir_all(&scratch).unwrap();
    let mut checked = 0;
    let mut failures = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("bx") {
            continue;
        }
        let out = burxt("check", &path, &scratch);
        checked += 1;
        if !out.status.success() {
            failures.push(format!(
                "{}: {}",
                path.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(checked >= 3, "expected several examples, checked only {}", checked);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Every rejection must say WHERE, and point somewhere real.
///
/// Run across all of tests/fail/, this catches the failure mode that would make
/// editor diagnostics useless: an error whose span was never set, which renders
/// as line 1 column 1 no matter where the mistake is. Most fail programs open
/// with a comment explaining what they test, so "the diagnostic points at a
/// comment or a blank line" is a reliable tell that the position is a default
/// rather than a fact.
#[test]
fn every_rejection_reports_a_position_that_points_at_code() {
    let scratch = scratch_dir("positions");
    fs::create_dir_all(&scratch).unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;

    for (program, _) in cases("fail", "stderr") {
        let src = fs::read_to_string(&program).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("check")
            .arg(&program)
            .arg("--json")
            .current_dir(&scratch)
            .output()
            .expect("failed to spawn burxt");
        let json = String::from_utf8_lossy(&out.stdout);
        checked += 1;

        // Minimal field extraction: the compiler has one dependency and a test
        // helper is not a reason for a second.
        let field = |name: &str| -> Option<usize> {
            let key = format!("\"{}\":", name);
            let at = json.find(&key)? + key.len();
            let rest = &json[at..];
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest[..end].parse().ok()
        };

        let (Some(line), Some(col)) = (field("line"), field("column")) else {
            failures.push(format!("{}: no position in {:?}", program.display(), json.trim()));
            continue;
        };

        let lines: Vec<&str> = src.lines().collect();
        if line == 0 || line > lines.len() {
            failures.push(format!(
                "{}: reported line {} but the file has {} lines",
                program.display(),
                line,
                lines.len()
            ));
            continue;
        }
        let text = lines[line - 1].trim();
        if text.is_empty() || text.starts_with("//") {
            failures.push(format!(
                "{}: points at line {} ({:?}) — a comment or blank line, so the span \
                 was probably never set",
                program.display(),
                line,
                lines[line - 1]
            ));
        }
        if col == 0 {
            failures.push(format!("{}: column 0 — columns are 1-based", program.display()));
        }
    }

    let _ = fs::remove_dir_all(&scratch);
    assert!(checked > 50, "expected the whole fail suite, checked {}", checked);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A real client session against `burxt lsp`: initialize, open a good file, break
/// it, fix it, shut down. Unit tests cover the framing and the diagnostic shape;
/// this covers the thing they cannot — that the process actually speaks the
/// protocol over a pipe, in order, and exits cleanly.
///
/// The sequence matters: publishing an EMPTY diagnostics array is what clears the
/// squiggle, so a server that only ever reports errors looks fine in a unit test
/// and leaves stale underlines in a real editor.
#[test]
fn language_server_publishes_and_clears_diagnostics() {
    use std::io::{Read, Write};
    use std::process::Stdio;

    let frame = |body: &str| format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let uri = "file:///tmp/burxt-lsp-probe.bx";
    let good = "let a: Int = 1;\\nprint(a);\\n";
    let bad = "let a: Int = 1;\\nlet b: Bool = 2;\\nprint(a);\\n";

    let mut session = String::new();
    session.push_str(&frame(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#,
    ));
    session.push_str(&frame(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#));
    session.push_str(&frame(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"burxt","version":1,"text":"{}"}}}}}}"#,
        uri, good
    )));
    session.push_str(&frame(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":2}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
        uri, bad
    )));
    session.push_str(&frame(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":3}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
        uri, good
    )));
    // Hover is supported: ask for the type of `a` in `print(a);` on line 2.
    session.push_str(&frame(&format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":1,"character":6}}}}}}"#,
        uri
    )));
    // An unknown request must still be answered, or a real client waits forever.
    session.push_str(&frame(r#"{"jsonrpc":"2.0","id":3,"method":"textDocument/definition","params":{}}"#));
    session.push_str(&frame(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#));
    session.push_str(&frame(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#));

    let mut child = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn burxt lsp");
    child.stdin.as_mut().unwrap().write_all(session.as_bytes()).unwrap();
    let mut out = String::new();
    child.stdout.as_mut().unwrap().read_to_string(&mut out).unwrap();
    let status = child.wait().unwrap();

    assert!(status.success(), "the server must exit cleanly after shutdown/exit");

    // Split the framed replies apart and keep the bodies.
    let bodies: Vec<&str> = out
        .split("Content-Length: ")
        .filter_map(|chunk| chunk.split_once("\r\n\r\n").map(|(_, body)| body))
        .collect();
    assert!(bodies.len() >= 6, "expected at least 6 messages, got {:?}", bodies);

    assert!(bodies[0].contains("\"textDocumentSync\":1"), "initialize reply: {}", bodies[0]);
    assert!(bodies[0].contains("\"hoverProvider\":true"), "initialize reply: {}", bodies[0]);
    assert!(bodies[0].contains("burxt-lsp"), "initialize reply should name the server");

    let published: Vec<&&str> = bodies
        .iter()
        .filter(|b| b.contains("publishDiagnostics"))
        .collect();
    assert_eq!(
        published.len(),
        3,
        "one publish per open/change, got {:?}",
        published
    );
    // Open (valid) -> empty. Change (broken) -> one error. Change back -> empty.
    assert!(published[0].contains("\"diagnostics\":[]"), "first: {}", published[0]);
    assert!(
        published[1].contains("declared Bool") && published[1].contains("\"severity\":1"),
        "second: {}",
        published[1]
    );
    // Line 2 of the file is line 1 to the protocol.
    assert!(published[1].contains("\"line\":1"), "second: {}", published[1]);
    assert!(
        published[2].contains("\"diagnostics\":[]"),
        "fixing the code must CLEAR the squiggle, got: {}",
        published[2]
    );

    // Hover answered with the type of `a`, as markdown.
    let hover = bodies
        .iter()
        .find(|b| b.contains("\"contents\""))
        .unwrap_or_else(|| panic!("no hover reply in {:?}", bodies));
    assert!(hover.contains("Int"), "hover should report `a: Int`: {}", hover);
    assert!(hover.contains("markdown"), "hover contents should be markdown: {}", hover);

    assert!(
        bodies.iter().any(|b| b.contains("-32601")),
        "an unsupported request must get a MethodNotFound reply, not silence"
    );
}

/// The `--json` diagnostic is a WIRE FORMAT with consumers outside this repo's
/// test suite: the VS Code extension reads it, and so will any CI gate. Renaming
/// a field would break them silently — the extension would simply stop showing
/// squiggles, with no error anywhere.
///
/// So this asserts the field names both ways: the compiler emits them, and
/// `editors/vscode/extension.js` reads the same ones. The second half is what
/// catches a rename that updates only one side.
#[test]
fn json_diagnostics_keep_the_contract_editors_depend_on() {
    let scratch = scratch_dir("json-contract");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("broken.bx");
    // Line 2, so a position of 0 would be indistinguishable from "unset".
    fs::write(&program, "let a: Int = 1;\nlet b: Bool = 2;\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("check")
        .arg(&program)
        .arg("--json")
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn burxt");
    let json = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let _ = fs::remove_dir_all(&scratch);

    assert!(!out.status.success(), "the probe program must be rejected");
    for field in [
        "\"file\":",
        "\"severity\":\"error\"",
        "\"message\":",
        "\"line\":",
        "\"column\":",
        "\"endLine\":",
        "\"endColumn\":",
        "\"lspStart\":",
        "\"lspEnd\":",
        "\"byteStart\":",
        "\"byteEnd\":",
    ] {
        assert!(json.contains(field), "missing {} in {}", field, json);
    }
    // The LSP positions are 0-based: the error is on file line 2, so line 1 here.
    // Character 14 is the `2` in `let b: Bool = 2;` — the caret blames the VALUE,
    // not the whole binding, because the declaration is not what is wrong.
    assert!(
        json.contains("\"lspStart\":{\"line\":1,\"character\":14}"),
        "0-based positions drifted: {}",
        json
    );
    // And exactly one JSON object, on one line, so a consumer can read it
    // line-by-line without a streaming parser.
    assert_eq!(json.lines().count(), 1, "one diagnostic per line: {}", json);

    // Now the consumer side: the extension must read the fields the compiler
    // writes. A rename on either side fails here.
    let ext = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/vscode/extension.js"),
    )
    .unwrap();
    for field in ["lspStart", "lspEnd", "message"] {
        assert!(
            ext.contains(field),
            "the VS Code extension does not read `{}`, which the compiler emits",
            field
        );
    }
    // It must invoke the stdin form, or it would check the file on disk and
    // report errors about code the user already fixed.
    assert!(
        ext.contains("\"check\", \"-\", \"--json\"") || ext.contains("'check', '-', '--json'"),
        "the extension must check the BUFFER via stdin (`check - --json`)"
    );
}
