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
fn record_layout_has_no_hidden_header() {
    let scratch = scratch_dir("layout");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("layout_probe.bx");
    fs::write(
        &program,
        "record Money { amount: Decimal<2> }\n\
         record LineItem { price: Decimal<2>, qty: Int }\n\
         record Order { total: Money, items: Int, label: String }\n\
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
fn dynamic_does_not_change_layout_and_costs_nothing_unused() {
    let scratch = scratch_dir("dynamic-layout");
    fs::create_dir_all(&scratch).unwrap();

    let common = "trait Priced { function price(self) -> Decimal<2> }\n\
                  record Book { cost: Decimal<2>, pages: Int }\n\
                  implement Priced for Book {\n\
                  function (self: Book) price() -> Decimal<2> { return self.cost; }\n\
                  }\n\
                  let b: Book = Book { cost: 1.00, pages: 2 };\n";

    let static_only = scratch.join("static_only.bx");
    fs::write(&static_only, format!("{}print(b.price());\n", common)).unwrap();

    let with_dyn = scratch.join("with_dyn.bx");
    fs::write(
        &with_dyn,
        format!("{}let d: dynamic Priced = b;\nprint(d.price());\n", common),
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
        "a program with no `dynamic` must emit no vtable"
    );
    assert!(
        d_ir.contains("bx.vtable.Priced.Book"),
        "a `dynamic` program must emit the (Type, Trait) vtable"
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
        "function down(n: Int, acc: Int) -> Int {\n\
         if n <= 0 { return acc; }\n\
         return tail down(n - 1, acc + 1);\n\
         }\n\
         function plain(n: Int, acc: Int) -> Int {\n\
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
        "external function record_cents(amount: Decimal<2> as scaled) -> Int;\n\
         external function take_double(n: CDouble) -> Int;\n\
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
        "external function take_double(n: CDouble) -> Int;\n\
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
/// Files under `examples/inputs/` are DATA for other examples to read, not programs.
/// They get their own test below, because two of them are wrong ON PURPOSE and silence
/// about that is what makes a reader think the repository is broken.
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

    // `--json` stays a supported interface for tasks and CI even though the VS
    // Code extension now speaks LSP instead: `.vscode/tasks.json` and the
    // `$burxt` problem matcher both depend on it.
}

/// The VS Code extension is a hand-written LSP client, and its failure modes are
/// the ones that look fine on inspection: a message split across chunks, a byte
/// length applied to a string, a promise that never resolves. `node
/// editors/vscode/test/harness.js` drives it against a real server with a stub
/// `vscode` module and checks the whole loop — diagnostics appearing, clearing,
/// and hover answering.
///
/// Run from here when node is available, and SKIPPED loudly when it is not: the
/// Rust suite must not require a JavaScript toolchain, but the check is too
/// valuable to leave un-run by default.
#[test]
fn vscode_extension_speaks_to_the_language_server() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let harness = root.join("editors/vscode/test/harness.js");
    let ext = fs::read_to_string(root.join("editors/vscode/extension.js")).unwrap();

    // The manifest has two properties that are easy to lose in an edit and whose
    // loss is silent: the language icon (v0.0.41 was spent chasing an icon that
    // was declared correctly all along) and the remote extension kind (without it,
    // the extension runs on the UI side of a WSL/SSH session and cannot see the
    // compiler at all).
    let manifest = fs::read_to_string(root.join("editors/vscode/package.json")).unwrap();
    for (needle, why) in [
        ("\"icon\"", "the extension must declare an icon for the burxt language"),
        // A language you cannot run from the editor is a language people read about
        // rather than try. The play button, the keybinding and the command behind them.
        ("burxt.run", "the extension must contribute a Run command"),
        ("editor/title/run", "Run must appear as the editor's play button"),
        ("ctrl+f5", "Run must have a keybinding"),
        ("file-icon.png", "the language icon file must be the one that is packaged"),
        ("\"extensionKind\"", "the extension must declare where it runs on a remote"),
        ("workspace", "extensionKind must be `workspace`: it spawns the compiler"),
    ] {
        assert!(manifest.contains(needle), "{}", why);
    }
    // The package version has to move when what it carries moves. VS Code keys its
    // upgrade on the version alone: a rebuilt `.vsix` with the same number installs as
    // "already installed", which is how a stale icon survives a reinstall (v0.0.69's
    // artwork swap did exactly that, and the installed copy kept the old mark).
    // Running a file must not leave a binary beside it: `-o` into a temp path is what
    // keeps a user's folder clean, and it is easy to drop in an edit.
    let extension_js = fs::read_to_string(root.join("editors/vscode/extension.js")).unwrap();
    assert!(
        extension_js.contains("os.tmpdir()") && extension_js.contains("-o "),
        "the Run command must build into a temp path, not the working directory"
    );

    assert!(
        !manifest.contains("\"version\": \"0.1.0\""),
        "bump the extension version when its contents change: VS Code will not reinstall \
         the same number"
    );

    // Everything the packager ships has to exist, or `pack.py` fails at the worst
    // possible moment — when someone is trying to install it.
    let packer = fs::read_to_string(root.join("editors/vscode/pack.py")).unwrap();
    let listed = packer
        .split("FILES = [")
        .nth(1)
        .and_then(|s| s.split(']').next())
        .expect("pack.py should list the files it packages");
    for line in listed.lines() {
        let name = line.trim().trim_end_matches(',').trim_matches('"');
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        assert!(
            root.join("editors/vscode").join(name).exists(),
            "pack.py packages `{}`, which does not exist",
            name
        );
    }

    // Static properties first, so a broken client is caught even without node.
    for (needle, why) in [
        ("\"lsp\"", "the extension must launch the language server"),
        ("publishDiagnostics", "it must apply the server's diagnostics"),
        ("registerHoverProvider", "it must offer hover"),
        ("Content-Length", "it must frame messages"),
        ("Buffer.concat", "it must buffer BYTES — a byte length applied to a string \
                           corrupts any message with a non-ASCII character"),
    ] {
        assert!(ext.contains(needle), "{}", why);
    }

    let node = Command::new("node").arg("--version").output();
    if node.map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!(
            "SKIPPED the live extension check: node is not available. \
             Run `node {}` where it is.",
            harness.display()
        );
        return;
    }

    let out = Command::new("node")
        .arg(&harness)
        .arg(env!("CARGO_BIN_EXE_burxt"))
        .current_dir(root)
        .output()
        .expect("failed to run node");
    assert!(
        out.status.success(),
        "the extension harness failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Every type error at once, in source order, with no invented ones.
///
/// The count matters as much as the messages: a checker that recovers badly
/// produces a cascade — one real mistake followed by five "unknown name" errors
/// about the binding it gave up on. Burxt avoids that because every `let`
/// declares its type, so a failed statement still contributes a correctly-typed
/// name. This asserts both halves: all the real errors, and nothing else.
#[test]
fn several_mistakes_are_all_reported_and_nothing_is_invented() {
    let scratch = scratch_dir("recovery");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("many.bx");
    fs::write(
        &program,
        // Three mistakes on lines 3, 5 and 7. Lines 8-9 USE the bindings whose
        // initializers failed, which is where a cascade would show up.
        "let price: Decimal<2> = $19.99;\n\
         let qty: Int = 3;\n\
         let wrong: Bool = qty;\n\
         let total: Decimal<2> = price * qty;\n\
         let bad: String = total;\n\
         let ok: Int = qty + 1;\n\
         let mixed: Int = price;\n\
         print(wrong);\n\
         print(bad);\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("check")
        .arg(&program)
        .arg("--json")
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn burxt");
    let json = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let _ = fs::remove_dir_all(&scratch);

    let lines: Vec<&str> = json.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "expected exactly the three real mistakes, got:\n{}",
        json
    );
    // In source order, so a reader can work top to bottom.
    for (line, wanted) in lines.iter().zip(["\"line\":3", "\"line\":5", "\"line\":7"]) {
        assert!(line.contains(wanted), "expected {} in {}", wanted, line);
    }
    assert!(
        !json.contains("unknown") && !json.contains("not declared"),
        "a failed `let` must still bind its DECLARED type, or later uses cascade:\n{}",
        json
    );
}

/// A lexer or parser error still arrives alone — recovering a token stream is its
/// own design question, and guessing where a malformed statement ends invents
/// errors rather than finding them. Asserted so the distinction stays deliberate.
#[test]
fn a_parse_error_is_reported_alone() {
    let scratch = scratch_dir("parse-alone");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("broken.bx");
    fs::write(&program, "let a: Int = 1\nlet b: Bool = 2;\nlet c: Int = \"x\";\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("check")
        .arg(&program)
        .arg("--json")
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn burxt");
    let json = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let _ = fs::remove_dir_all(&scratch);

    assert_eq!(json.lines().count(), 1, "one parse error, reported once:\n{}", json);
    assert!(json.contains("expected"), "{}", json);
}

/// The stage-1 front end, written in Burxt, must LEX AND PARSE every Burxt source in
/// this repository without an error — including its own source, which is the first
/// real test of a self-hosted compiler.
///
/// It is a cross-check, not a unit test: the Rust lexer already accepted these files
/// (they compile), so any byte the Burxt lexer refuses is a disagreement between the
/// two, and one of them is wrong.
#[test]
fn the_burxt_front_end_accepts_every_burxt_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("stage1-lexer");
    fs::create_dir_all(&scratch).unwrap();

    // Build it once with the Rust compiler, then run it over everything.
    let build = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn burxt");
    assert!(
        build.status.success(),
        "the stage-1 lexer did not compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let mut sources: Vec<PathBuf> = vec![
        root.join("examples/stage1.bx"),
        root.join("examples/checker.bx"),
        root.join("examples/symbols.bx"),
        root.join("examples/lexer.bx"),
        root.join("examples/parser.bx"),
        root.join("examples/tour.bx"),
        root.join("examples/money.bx"),
    ];
    // Plus every program the suite already accepts.
    for (program, _) in cases("pass", "stdout") {
        sources.push(program);
    }

    let mut failures = Vec::new();
    for source in &sources {
        let out = Command::new(scratch.join("stage1"))
            .arg(source)
            .current_dir(&scratch)
            .output()
            .expect("failed to run the stage-1 lexer");
        let text = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() {
            failures.push(format!("{}: exited {:?}: {}", source.display(), out.status.code(), stderr));
            continue;
        }
        // Both halves report their own failures: bytes that started no token, and
        // constructs the parser could not read.
        if !text.contains("errors:      0") {
            failures.push(format!(
                "{}: the Burxt LEXER reported an error the Rust lexer did not:\n{}",
                source.display(),
                text
            ));
        }
        if !text.contains("parse errors: 0") {
            failures.push(format!(
                "{}: the Burxt PARSER reported an error the Rust parser did not:\n{}",
                source.display(),
                text
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(
        sources.len() > 50,
        "expected to cross-check the whole pass suite, got {}",
        sources.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The stage-1 TYPECHECKER, written in Burxt, must refuse what stage-0 refuses and
/// accept what stage-0 accepts — over the subset it covers so far.
///
/// Two directions, because either one alone is easy to pass: a checker that says
/// nothing accepts everything, and a checker that says everything catches everything.
#[test]
fn the_burxt_typechecker_agrees_with_the_rust_one() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("stage1-check");
    fs::create_dir_all(&scratch).unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn burxt");
    assert!(build.status.success(), "stage-1 did not compile");

    let errors_reported = |file: &Path| -> i32 {
        let out = Command::new(scratch.join("stage1"))
            .arg(file)
            .current_dir(&scratch)
            .output()
            .expect("failed to run stage-1");
        let text = String::from_utf8_lossy(&out.stdout);
        text.lines()
            .find_map(|l| l.trim().strip_prefix("type errors: "))
            .and_then(|n| n.parse().ok())
            .unwrap_or(-1)
    };

    // Direction 1: silent on every program stage-0 accepts. Not a sample — the whole
    // pass suite, plus its own source, which is the strongest single case at 2,300
    // lines of the language. A false positive here means the two implementations
    // disagree about what Burxt IS.
    let mut noisy = Vec::new();
    for name in ["examples/stage1.bx", "examples/tour.bx", "examples/money.bx"] {
        if errors_reported(&root.join(name)) != 0 {
            noisy.push(name.to_string());
        }
    }
    for entry in fs::read_dir(root.join("tests/pass")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("bx") {
            continue;
        }
        if errors_reported(&path) != 0 {
            noisy.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        noisy.is_empty(),
        "stage-1 complained about programs stage-0 accepts: {:?}",
        noisy
    );

    // Direction 2: it catches the mistakes stage-0 catches. One program, every rule
    // this phase implements.
    let wrong = scratch.join("wrong.bx");
    fs::write(
        &wrong,
        "function tax(amount: Decimal<2>, rate: Decimal<4>) -> Decimal<2, RoundHalfEven> {\n\
         return amount * rate;\n\
         }\n\
         let price: Decimal<2> = $19.99;\n\
         let rate: Decimal<4> = 8.25%;\n\
         let scales: Decimal<2> = price + rate;\n\
         let narrow: Int = price;\n\
         let arity: Decimal<2, RoundHalfEven> = tax(price);\n\
         let contract: Decimal<2> = price * rate;\n\
         let truth: Bool = price;\n\
         let divided: Int = 7 / 2;\n\
         print(nobody_declared_this);\n",
    )
    .unwrap();
    let found = errors_reported(&wrong);

    // Direction 3: the rules v0.0.59 added — a match arm's bindings against the
    // variant's payload, an element type read through an index, and the String that
    // is deliberately not indexable.
    let shapes = scratch.join("shapes.bx");
    fs::write(
        &shapes,
        "enum Step { Go(Int), Stop }\n\
         let s: Step = Step.Go(2);\n\
         match s {\n\
         Go(a, b) => { print(\"two\"); }\n\
         Stop => { print(\"stop\"); }\n\
         }\n\
         let xs: [Int; 3] = [1, 2, 3];\n\
         let bad: String = xs[0];\n\
         let text: String = \"hi\";\n\
         print(to_string(text[0]));\n",
    )
    .unwrap();
    let shape_errors = errors_reported(&shapes);

    // Direction 4: a container's ELEMENT type is part of its type. Stage-1 compared two
    // slices by falling through to "the same" — its `ty_same` is a free function and a
    // slice holds its element as a node index, which needs the arena to read — so
    // `[Int]` and `[String]` were interchangeable for eleven versions and it accepted
    // `takes_ints(words)` where stage-0 refused. The rejection ratchet below is a FLOOR,
    // so rejecting less than stage-0 kept it green. Found by trying to build generic
    // unification on top of it, which needed element comparison to mean something.
    let elements = scratch.join("elements.bx");
    fs::write(
        &elements,
        "function takes_ints(xs: [Int]) -> Int { return len(xs); }\n\
         region r {\n\
         let mutable words: [String] = [];\n\
         let a = push(words, \"one\");\n\
         print(takes_ints(words));\n\
         }\n",
    )
    .unwrap();
    let element_errors = errors_reported(&elements);

    // Direction 2b, a ratchet: how much of the fail suite stage-1 rejects on its own.
    // It is not all of it — regions, purity, exhaustiveness and the reserved names are
    // still partly stage-0's alone — so the number is a floor that may only go up. A floor
    // rather than an exact count, because catching MORE is the goal, not a regression.
    //
    // The floor being a floor is also how the element-type bug above hid for eleven
    // versions: stage-1 rejecting LESS than stage-0 never trips it. Worth knowing about the
    // instrument — a ratchet measures progress, and cannot notice a regression that stays
    // above the line. Direction 4 is the kind of test that can.
    //
    // The floor moved DOWN once, from 192 to 189 in v0.0.107, and that is the only lowering
    // in its history. Before that version stage-1 refused EVERY program containing a generic
    // with one blanket message, so `generic_record_needs_its_arguments` and
    // `generic_enum_needs_its_arguments` counted as rejections — for the wrong reason.
    // Now generic FUNCTIONS are checked properly and those two were honestly out of scope
    // until generic records and enums were instantiated. Three fixtures traded from
    // accidentally-right to knowingly-pending, written down rather than quietly absorbed.
    //
    // v0.0.108 raised it to 190 and earned the three back for the right reason: the arity and
    // not-generic rules over every type application, the generic-`external function` refusal,
    // and the message for a generic named with nothing to infer from. One fixture was still
    // knowingly out of scope, `generic_enum_payload_must_be_scalar`, because it is a rule about
    // an instantiated enum's LAYOUT.
    //
    // v0.0.109 raised it to 191 by closing that one: layout now resolves through the arguments
    // in scope, so stage-1 can ask what an instantiated payload actually is. The list of
    // knowingly-pending fixtures is empty, which is the first time that has been true.
    let mut caught = 0;
    let mut total = 0;
    for entry in fs::read_dir(root.join("tests/fail")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("bx") {
            continue;
        }
        total += 1;
        if errors_reported(&path) != 0 {
            caught += 1;
        }
    }

    let _ = fs::remove_dir_all(&scratch);
    assert!(
        caught >= 191,
        "stage-1 rejected only {} of {} fail programs, down from 191",
        caught,
        total
    );
    assert_eq!(
        element_errors, 1,
        "stage-1 must reject a [String] where a [Int] is wanted — an element type is part \
         of the type, and comparing two slices as equal made them interchangeable"
    );
    assert_eq!(
        shape_errors, 4,
        "stage-1 should have caught the arity, the element type, the indexed String, \
         and the to_string with no region open"
    );
    assert_eq!(
        found, 7,
        "expected stage-1 to catch all seven mistakes and invent none, got {}",
        found
    );
}

/// The proof that phase 5 is real: a program compiled by the compiler written IN
/// BURXT runs, and prints exactly what stage-0's build of the same source prints.
///
/// Stage-1 writes textual LLVM IR — M4 §1's decision, forced by `extern fn` returns
/// being Int and CInt only, so an LLVMBuilderRef is unreachable by construction. `llc`
/// and the system linker turn that text into a program, which is the same path any
/// other compiler's output takes.
#[test]
fn programs_compiled_by_the_burxt_backend_run_and_agree_with_stage_0() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    if !llc.exists() {
        eprintln!("skipping: {} is not installed", llc.display());
        return;
    }
    let scratch = scratch_dir("stage1-backend");
    fs::create_dir_all(&scratch).unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn burxt");
    assert!(build.status.success(), "stage-1 did not compile");

    // What slice 1 covers: Ints, Bools, String literals, checked arithmetic,
    // comparisons, `if`, `while`, `break`, `continue`, functions, calls, `print`.
    let programs: [(&str, &str); 7] = [
        ("arith.bx", "let a: Int = 6;\nlet b: Int = 7;\nprint(a * b);\nprint(a - b);\n"),
        (
            "loop.bx",
            "let mutable i: Int = 0;\nwhile i < 4 {\n  if i == 2 { i = i + 1; continue; }\n               print(i * 10);\n  i = i + 1;\n}\nprint(\"end\");\n",
        ),
        (
            "calls.bx",
            "function fact(n: Int) -> Int {\n  if n <= 1 { return 1; }\n  return n * fact(n - 1);\n}\n\
             print(fact(6));\n",
        ),
        (
            "logic.bx",
            "let n: Int = 5;\nprint(n >= 5 && n <= 5);\nprint(n != 5 || false);\nprint(!true);\n",
        ),
        // Strings, the region they are built in, and the `allocates` function that
        // hands one back — M1a's whole claim, checked by running it.
        (
            "strings.bx",
            "function describe(line: Int) -> String allocates {\n               return \"line \" + to_string(line) + \": unexpected byte\";\n}\n             region r {\n  print(describe(3));\n  let s: String = \"hello, burxt\";\n               print(len(s));\n  print(byte_at(s, 0));\n  print(substring(s, 7, 5));\n               print(to_string(true) + \"/\" + to_string(false));\n}\n",
        ),
        // Structs by value, a nested struct, a struct-typed parameter, a fixed array
        // read and written, and an aggregate copied — `b = a` then `b.x = 100` must
        // leave `a.x` alone, which is the whole of by-value semantics.
        (
            "aggregates.bx",
            "record Point { x: Int, y: Int }\n             record Line { from: Point, to: Point, label: String }\n             function total_of(p: Point) -> Int { return p.x + p.y; }\n             let a: Point = Point { x: 3, y: 4 };\n             let mutable b: Point = a;\n             b.x = 100;\n             print(total_of(a));\nprint(a.x);\nprint(b.x);\n             let l: Line = Line { from: a, to: b, label: \"diagonal\" };\n             print(l.from.x);\nprint(l.to.x);\nprint(l.label);\n             let mutable xs: [Int; 4] = [10, 20, 30, 40];\n             xs[1] = 99;\n             let mutable i: Int = 0;\nlet mutable total: Int = 0;\n             while i < 4 { total = total + xs[i]; i = i + 1; }\n             print(total);\n",
        ),
        (
            "division.bx",
            "print(divide_floor(-7, 2));\nprint(divide_toward_zero(-7, 2));\nprint(remainder(-7, 2));\n             print(divide_floor(7, 2));\n",
        ),
    ];

    let mut failures = Vec::new();
    for (name, source) in programs {
        let src = scratch.join(name);
        fs::write(&src, source).unwrap();

        // stage-0's answer, which is the one to match.
        let expected = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("run")
            .arg(&src)
            .current_dir(&scratch)
            .output()
            .expect("failed to run stage-0");
        let expected_out = String::from_utf8_lossy(&expected.stdout)
            .lines()
            .filter(|l| !l.starts_with("compiled "))
            .map(|l| format!("{}\n", l))
            .collect::<String>();

        // stage-1: source -> IR text -> object -> program.
        let ll = scratch.join(format!("{}.ll", name));
        let emit = Command::new(scratch.join("stage1"))
            .arg(&src)
            .arg(&ll)
            .current_dir(&scratch)
            .output()
            .expect("failed to run stage-1");
        let emit_out = String::from_utf8_lossy(&emit.stdout);
        if !emit_out.contains("bytes of IR") {
            failures.push(format!("{}: stage-1 emitted nothing\n{}", name, emit_out));
            continue;
        }
        let obj = scratch.join(format!("{}.o", name));
        let compiled = Command::new(llc)
            .args(["-relocation-model=pic", "-filetype=obj", "-o"])
            .arg(&obj)
            .arg(&ll)
            .output()
            .expect("failed to run llc");
        if !compiled.status.success() {
            failures.push(format!(
                "{}: llc rejected the IR\n{}",
                name,
                String::from_utf8_lossy(&compiled.stderr)
            ));
            continue;
        }
        let exe = scratch.join(format!("{}.exe", name));
        let linked = Command::new("cc").arg("-o").arg(&exe).arg(&obj).output().expect("cc");
        if !linked.status.success() {
            failures.push(format!(
                "{}: link failed\n{}",
                name,
                String::from_utf8_lossy(&linked.stderr)
            ));
            continue;
        }
        let ran = Command::new(&exe).output().expect("failed to run the program");
        let got = String::from_utf8_lossy(&ran.stdout).to_string();
        if got != expected_out {
            failures.push(format!(
                "{}: stage-1's program printed {:?}, stage-0's printed {:?}",
                name, got, expected_out
            ));
        }
    }

    // A named runtime failure: exit 70, the message on stderr, and the output before it
    // intact. A bounds check that is only a comment is not a bounds check.
    let oob = scratch.join("oob.bx");
    fs::write(
        &oob,
        "let xs: [Int; 3] = [1, 2, 3];\nlet mutable i: Int = 0;\n         while i < 5 { print(xs[i]); i = i + 1; }\n",
    )
    .unwrap();
    let ll = scratch.join("oob.ll");
    let emitted = Command::new(scratch.join("stage1"))
        .arg(&oob)
        .arg(&ll)
        .current_dir(&scratch)
        .output()
        .expect("stage-1");
    assert!(
        String::from_utf8_lossy(&emitted.stdout).contains("bytes of IR"),
        "stage-1 did not emit the bounds program"
    );
    let obj = scratch.join("oob.o");
    assert!(Command::new(llc)
        .args(["-relocation-model=pic", "-filetype=obj", "-o"])
        .arg(&obj)
        .arg(&ll)
        .status()
        .expect("llc")
        .success());
    let exe = scratch.join("oob.exe");
    assert!(Command::new("cc")
        .arg("-o")
        .arg(&exe)
        .arg(&obj)
        .status()
        .expect("cc")
        .success());
    let ran = Command::new(&exe).output().expect("run");
    let out = String::from_utf8_lossy(&ran.stdout).to_string();
    let err = String::from_utf8_lossy(&ran.stderr).to_string();
    let code = ran.status.code();

    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(out, "1\n2\n3\n", "the reads before the bad one must still print");
    // The message carries the numbers and the position, because "outside the array" left
    // a reader to guess which index and how long the array was — and the two together
    // are usually the whole diagnosis. That improvement is what found the short-circuit
    // bug in v0.0.73, so it is asserted rather than assumed.
    assert!(
        err.contains("index 3 is outside an array of 3"),
        "the failure belongs on stderr, with the index and the length: {:?}",
        err
    );
    assert!(err.contains("(at byte "), "and with the position in the source: {:?}", err);
    assert_eq!(code, Some(70), "a named runtime failure exits 70");
}

/// **Burxt compiles Burxt, and the result is fixed.** The self-hosting certificate, run
/// end to end on every `cargo test`:
///
/// 1. stage-0 (this Rust compiler) builds **stage-1** from `examples/stage1.bx`.
/// 2. stage-1 emits LLVM IR for **its own source**, with no construct refused.
/// 3. That IR is assembled and linked into **stage-2** — a Burxt compiler built by a
///    Burxt compiler.
/// 4. stage-2 emits IR for the same source, and it must be **byte-identical** to
///    stage-1's. That is the fixpoint: the compiler has stopped changing its own output,
///    which is what says the two implementations agree about the whole language they
///    share, not just about the programs someone thought to test.
/// 5. stage-2 answers exactly what stage-1 answers for every program in the pass suite.
///
/// What this does NOT claim: that stage-1 can compile every Burxt program. Its backend
/// does not emit Decimals, `match`, `tail` or contracts yet — none of which its own
/// source uses. The certificate is that it compiles ITSELF and reaches a fixpoint.
#[test]
fn burxt_compiles_burxt_and_reaches_the_fixpoint() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    if !llc.exists() {
        eprintln!("skipping: {} is not installed", llc.display());
        return;
    }
    let scratch = scratch_dir("fixpoint");
    fs::create_dir_all(&scratch).unwrap();
    let stage1 = scratch.join("stage1");
    assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .expect("burxt")
        .success());

    // A Burxt program from a Burxt compiler: source -> IR text -> object -> program.
    let build_stage = |compiler: &Path, ir: &PathBuf, exe: &PathBuf| -> String {
        let emitted = Command::new(compiler)
            .arg(root.join("examples/stage1.bx"))
            .arg(ir)
            .output()
            .expect("a compiler");
        let said = String::from_utf8_lossy(&emitted.stdout).to_string();
        assert!(
            said.contains("bytes of IR") && !said.contains("backend refusals"),
            "{} could not emit stage-1's source:\n{}",
            compiler.display(),
            said
        );
        let obj = ir.with_extension("o");
        let compiled = Command::new(llc)
            .args(["-relocation-model=pic", "-filetype=obj", "-o"])
            .arg(&obj)
            .arg(ir)
            .output()
            .expect("llc");
        assert!(
            compiled.status.success(),
            "llc rejected the IR from {}:\n{}",
            compiler.display(),
            String::from_utf8_lossy(&compiled.stderr)
        );
        assert!(Command::new("cc")
            .arg("-o")
            .arg(exe)
            .arg(&obj)
            .status()
            .expect("cc")
            .success());
        said
    };

    let ir1 = scratch.join("self.ll");
    let stage2 = scratch.join("stage2");
    build_stage(&stage1, &ir1, &stage2);

    let ir2 = scratch.join("self2.ll");
    let stage3 = scratch.join("stage3");
    build_stage(&stage2, &ir2, &stage3);

    let first = fs::read(&ir1).unwrap();
    let second = fs::read(&ir2).unwrap();

    // Every program in the pass suite, through both compilers.
    let mut disagreements = Vec::new();
    let mut checked = 0;
    for entry in fs::read_dir(root.join("tests/pass")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("bx") {
            continue;
        }
        checked += 1;
            let one = Command::new(&stage1)
            .arg(&path)
            .current_dir(&scratch)
            .output()
            .expect("stage-1");
        let two = Command::new(&stage2)
            .arg(&path)
            .current_dir(&scratch)
            .output()
            .expect("stage-2");
        if one.stdout != two.stdout {
            disagreements.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }

    let _ = fs::remove_dir_all(&scratch);
    assert_eq!(
        first.len(),
        second.len(),
        "stage-2's output is a different SIZE from stage-1's: no fixpoint"
    );
    assert!(
        first == second,
        "stage-1 and stage-2 emit different IR for the same source: no fixpoint"
    );
    assert!(
        disagreements.is_empty(),
        "stage-2 disagreed with stage-1 about {} of {} programs: {:?}",
        disagreements.len(),
        checked,
        disagreements
    );
    assert!(checked >= 88, "the pass suite shrank: only {} programs", checked);
}

/// The repository root holds only what belongs there. Not a style preference: `burxt
/// build` writes a bare, extensionless executable into the working directory, so a
/// compiler that is exercised by hand from its own root accumulates them — twenty-six
/// of them, by v0.0.70, next to two `.ll` files that had been committed by accident and
/// two demo programs that belonged in `examples/`.
///
/// An allowlist rather than a pattern, because the question "should this be at the root?"
/// has a short, knowable answer, and anything new has to be added deliberately.
#[test]
fn the_repository_root_holds_only_what_belongs_there() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    const ALLOWED: [&str; 15] = [
        // build system and metadata
        "Cargo.toml",
        "Cargo.lock",
        ".cargo",
        ".git",
        ".gitignore",
        ".gitattributes",
        ".vscode",
        "target",
        // the documents a reader looks for first
        "README.md",
        "DESIGN.md",
        "CONTRIBUTING.md",
        "CODE_OF_CONDUCT.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        // everything else lives in a directory that says what it is
        "",
    ];
    const DIRS: [&str; 12] = [
        "src", "spec", "tests", "examples", "editors", "assets", "docs", "lib", "scripts",
        // built by scripts/release.sh, ignored by git, never committed
        "dist",
        // CI and the release workflow, and the Codespaces container. GitHub fixes both names, so
        // they are the two directories here whose spelling is not ours to choose.
        ".github",
        ".devcontainer",
    ];

    let mut strays = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if ALLOWED.contains(&name.as_str()) || DIRS.contains(&name.as_str()) {
            continue;
        }
        strays.push(name);
    }
    strays.sort();
    assert!(
        strays.is_empty(),
        "these do not belong at the repository root — build with `-o <path>` and move \
         sources into a directory: {:?}",
        strays
    );
}

/// The milestone log is split across files, so the index and the files must agree. Two
/// ways to drift, both silent: a log file nobody links to, and a link to a file that was
/// renamed. Checked because the log's whole value is that an entry can be found later.
#[test]
fn every_log_file_is_linked_from_its_index() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("docs/log");
    let index = fs::read_to_string(dir.join("README.md")).expect("docs/log/README.md");

    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") && name != "README.md" {
            files.push(name);
        }
    }
    assert!(files.len() >= 8, "the log lost files: {:?}", files);
    for name in &files {
        assert!(index.contains(name.as_str()), "docs/log/README.md does not link {}", name);
    }
    // And the other direction: every link in the index resolves.
    for piece in index.split('(').skip(1) {
        let target = piece.split(')').next().unwrap_or("");
        if target.ends_with(".md") && !target.starts_with("../") && !target.contains('#') {
            assert!(
                dir.join(target).exists(),
                "docs/log/README.md links {}, which does not exist",
                target
            );
        }
    }
    // DESIGN.md must point at the log rather than holding it: the split is the point.
    let design = fs::read_to_string(root.join("DESIGN.md")).unwrap();
    assert!(design.contains("docs/log/"), "DESIGN.md must link the log");
    assert!(
        design.lines().count() < 1200,
        "DESIGN.md is growing back into a log: {} lines",
        design.lines().count()
    );
}

/// Two directories of data, and the DIRECTORY is the promise: everything in
/// `examples/negative/` must be rejected, everything in `examples/inputs/` must compile.
///
/// A folder rather than a naming convention, because a folder answers the question
/// before anyone opens a file — and because a rule holds for files nobody has written
/// yet, while a list of exceptions rots. Without this test, "fixing" a negative input
/// would quietly turn a demonstration into a demonstration of nothing, with the suite
/// still green.
#[test]
fn the_negative_examples_are_still_negative() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("negative");
    fs::create_dir_all(&scratch).unwrap();
    let mut wrong = Vec::new();
    let mut counted = 0;

    for (dir, must_fail) in [("examples/negative", true), ("examples/inputs", false)] {
        let path = root.join(dir);
        let mut seen = 0;
        for entry in fs::read_dir(&path).unwrap() {
            let file = entry.unwrap().path();
            if file.extension().and_then(|e| e.to_str()) != Some("bx") {
                continue;
            }
            seen += 1;
            counted += 1;
            let rejected = !burxt("check", &file, &scratch).status.success();
            if rejected != must_fail {
                wrong.push(format!(
                    "{}/{}: the directory says it must {}, but the compiler {} it",
                    dir,
                    file.file_name().unwrap().to_string_lossy(),
                    if must_fail { "fail" } else { "compile" },
                    if rejected { "rejected" } else { "accepted" }
                ));
            }
        }
        assert!(seen > 0, "{} has no .bx files left", dir);
        // Each directory explains itself, because the first person to meet these files is
        // someone who opened one and saw red squiggles.
        assert!(
            path.join("README.md").exists(),
            "{} must say what it is: no README.md",
            dir
        );
    }

    let _ = fs::remove_dir_all(&scratch);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(counted >= 4, "the example data shrank: {} files", counted);
}

/// The guide and the examples are only useful if they are reachable and true. Three ways
/// they rot silently: a page nobody links to, an example nobody lists, and an example that
/// stopped compiling. All three are checked here.
#[test]
fn the_guide_and_examples_are_linked_and_compile() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("guide");
    fs::create_dir_all(&scratch).unwrap();

    // Every guide page is linked from the guide's index, and the README points at the guide.
    //
    // There are TWO indexes, and they are a translation rather than a duplication: `README.md` links
    // `.md` files for someone reading the repository, and `index.md` links `.html` for someone on
    // the site, because Jekyll renames them. Neither is a page, so neither has to appear in the
    // other — but each must reach every real page, and `the_site_is_honest_and_complete` holds
    // index.md to exactly this rule so the pair cannot fall out of step.
    let index = fs::read_to_string(root.join("docs/guide/README.md")).expect("guide index");
    let mut pages = 0;
    for entry in fs::read_dir(root.join("docs/guide")).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") && name != "README.md" && name != "index.md" {
            pages += 1;
            assert!(index.contains(&name), "docs/guide/README.md does not link {}", name);
        }
    }
    assert!(pages >= 11, "the guide lost pages: {} left", pages);
    // Every `burxt` code block in the guide and the README must use the language's CURRENT
    // spelling. This is the rot that actually happened: v0.0.98 renamed six keywords, and prose
    // does not fail to compile, so a page can go on teaching `fn` and `struct` indefinitely while
    // every test stays green.
    //
    // What this does NOT check is that the blocks compile. Most are fragments — three lines of a
    // record, a call with no declaration in sight — and wrapping them in a guessed context would
    // fail for reasons the guide is not wrong about. The examples are the compiled artefact, and
    // every page points at one.
    let stale: &[(&str, &str)] = &[
        ("fn ", "function"),
        ("struct ", "record"),
        ("impl ", "implement"),
        ("mut ", "mutable"),
        ("extern ", "external function"),
        (": ty", ": type"),
    ];
    let mut prose = vec![root.join("README.md")];
    for entry in fs::read_dir(root.join("docs/guide")).unwrap() {
        prose.push(entry.unwrap().path());
    }
    for path in &prose {
        let text = fs::read_to_string(path).unwrap();
        for block in text.split("```burxt").skip(1) {
            let code = block.split("```").next().unwrap_or("");
            for (old_spelling, now) in stale {
                assert!(
                    !code.contains(old_spelling),
                    "{} teaches `{}`, which the language renamed to `{}`:\n{}",
                    path.display(),
                    old_spelling.trim(),
                    now,
                    code.trim()
                );
            }
        }
    }

    let readme = fs::read_to_string(root.join("README.md")).unwrap();
    assert!(readme.contains("docs/guide/"), "README.md must link the guide");
    assert!(readme.contains("examples/"), "README.md must link the examples");

    // Every example is listed in the examples index, and still compiles.
    let listing = fs::read_to_string(root.join("examples/README.md")).expect("examples index");
    let mut failures = Vec::new();
    let mut counted = 0;
    for entry in fs::read_dir(root.join("examples")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("bx") {
            continue;
        }
        counted += 1;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !listing.contains(&name) {
            failures.push(format!("examples/README.md does not list {}", name));
        }
        let out = burxt("check", &path, &scratch);
        if !out.status.success() {
            failures.push(format!(
                "{} no longer compiles:\n{}",
                name,
                String::from_utf8_lossy(&out.stdout)
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(counted >= 13, "examples went missing: {} left", counted);
}

/// How much of the language the BURXT backend can actually compile — not "does it emit
/// something", but "does the program it produces print what stage-0's prints". Every
/// program in the pass suite is compiled by stage-1, assembled, linked, run, and diffed
/// against its recorded output.
///
/// **This reached 88 of 88 in v0.0.79**: the Burxt compiler builds every program the Rust
/// one can, and each program prints the same bytes either way. The number got there as a
/// ratchet — 31 with no Decimals, 58 with them, 77 with enums, `match`, interpolation,
/// `extern fn` and `musttail`, 88 once `dyn` dispatch and three narrow defects were done —
/// and it stays one, because going backwards is the failure this guards against.
#[test]
fn the_burxt_backend_compiles_a_growing_share_of_the_suite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    if !llc.exists() {
        eprintln!("skipping: {} is not installed", llc.display());
        return;
    }
    let scratch = scratch_dir("backend-share");
    fs::create_dir_all(&scratch).unwrap();
    let stage1 = scratch.join("stage1");
    assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .expect("burxt")
        .success());

    // A program may read a file next to it, so the fixtures travel to the scratch
    // directory with it — `read_file.bx` wants `source_fixture.txt`.
    for entry in fs::read_dir(root.join("tests/pass")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("txt") {
            let _ = fs::copy(&path, scratch.join(path.file_name().unwrap()));
        }
    }

    let mut correct = 0;
    let mut total = 0;
    let mut refused = 0;
    for entry in fs::read_dir(root.join("tests/pass")).unwrap() {
        let source = entry.unwrap().path();
        if source.extension().and_then(|e| e.to_str()) != Some("bx") {
            continue;
        }
        let expected_path = source.with_extension("stdout");
        if !expected_path.exists() {
            continue;
        }
        total += 1;
        let ll = scratch.join("out.ll");
        let emitted = Command::new(&stage1).arg(&source).arg(&ll).output().expect("stage-1");
        if !String::from_utf8_lossy(&emitted.stdout).contains("bytes of IR") {
            refused += 1;
            continue;
        }
        let obj = scratch.join("out.o");
        if !Command::new(llc)
            .args(["-relocation-model=pic", "-filetype=obj", "-o"])
            .arg(&obj)
            .arg(&ll)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            continue;
        }
        let exe = scratch.join("out.exe");
        if !Command::new("cc").arg("-o").arg(&exe).arg(&obj).status().map(|s| s.success()).unwrap_or(false) {
            continue;
        }
        // In the scratch directory, because a program under test may WRITE a file —
        // `driver_primitives.bx` does — and it must not land in the repository.
        let ran = Command::new(&exe)
            .current_dir(&scratch)
            .output()
            .expect("the program");
        let expected = fs::read(&expected_path).unwrap();
        if ran.stdout == expected {
            correct += 1;
        }
    }

    let _ = fs::remove_dir_all(&scratch);
    eprintln!(
        "the Burxt backend compiles {} of {} pass programs correctly ({} refused outright)",
        correct, total, refused
    );
    // **ALL of them, since v0.0.113.** The ratchet has run out of room, which means it stops
    // being a ratchet and becomes an equality: the Burxt backend compiles every program the Rust
    // one does, and each prints the same bytes either way. A floor was the right instrument while
    // there was a gap to close; keeping one now would let a regression hide above the line, which
    // is the mistake `Direction 4` was added to fix elsewhere in this file.
    //
    // It got here as a ratchet: 31 with no Decimals, 58 with them, 77 with enums, `match`,
    // interpolation, `external function` and `musttail`, 88 once `dynamic` dispatch landed, 98
    // with generic records and enums, 101 with generic functions and methods, 102 with
    // `write_bytes`.
    assert_eq!(
        correct, total,
        "the Burxt backend compiled {} of {} pass programs. It compiled ALL of them from \
         v0.0.113, so this is a regression, and `refused` was {}",
        correct, total, refused
    );
}

/// **The suite, run by Burxt.** `tests/runner.bx` walks the same fixtures this file walks
/// — pass, fail and panic — and reports the same verdict. A second implementation of the
/// harness, standing to this one exactly as stage-1 stands to stage-0: not a replacement,
/// a cross-check, so a fixture cannot quietly mean two different things.
///
/// It needs nothing new from the language. Burxt cannot list a directory — `opendir`
/// returns a pointer and the memory model has nothing to say about who owns it — so the
/// shell lists it and the answer comes back through a file. An honest limit, worked
/// around in the open, and the first thing the standard library will hide.
#[test]
fn the_suite_also_runs_on_burxt() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("burxt-runner");
    fs::create_dir_all(&scratch).unwrap();
    let runner = scratch.join("runner");
    let built = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("tests/runner.bx"))
        .arg("-o")
        .arg(&runner)
        .output()
        .expect("burxt");
    assert!(
        built.status.success(),
        "the Burxt test runner did not compile:\n{}",
        String::from_utf8_lossy(&built.stdout)
    );

    // From the repository root, because the fixtures are named relative to it.
    let ran = Command::new(&runner)
        .arg(env!("CARGO_BIN_EXE_burxt"))
        .arg(scratch.join("work"))
        .current_dir(root)
        .output()
        .expect("the Burxt runner");
    let said = String::from_utf8_lossy(&ran.stdout).to_string();

    // How many fixtures this file would check, counted the same way.
    let mut fixtures = 0;
    for kind in ["pass", "fail", "panic"] {
        for entry in fs::read_dir(root.join("tests").join(kind)).unwrap() {
            if entry.unwrap().path().extension().and_then(|e| e.to_str()) == Some("bx") {
                fixtures += 1;
            }
        }
    }

    // And the whole loop: the runner, written in Burxt, compiled by the compiler written
    // in Burxt, checking the compiler written in Rust. Burxt by Burxt, with stage-0 as the
    // thing under test rather than the thing doing the testing.
    let mut selfhosted = String::new();
    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    if llc.exists() {
        let stage1 = scratch.join("stage1");
        assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("build")
            .arg(root.join("examples/stage1.bx"))
            .arg("-o")
            .arg(&stage1)
            .status()
            .expect("burxt")
            .success());
        let ll = scratch.join("runner.ll");
        let emitted = Command::new(&stage1)
            .arg(root.join("tests/runner.bx"))
            .arg(&ll)
            .output()
            .expect("stage-1");
        assert!(
            String::from_utf8_lossy(&emitted.stdout).contains("bytes of IR"),
            "stage-1 could not emit the test runner:\n{}",
            String::from_utf8_lossy(&emitted.stdout)
        );
        let obj = scratch.join("runner.o");
        assert!(Command::new(llc)
            .args(["-relocation-model=pic", "-filetype=obj", "-o"])
            .arg(&obj)
            .arg(&ll)
            .status()
            .expect("llc")
            .success());
        let native = scratch.join("runner-by-burxt");
        assert!(Command::new("cc")
            .arg("-o")
            .arg(&native)
            .arg(&obj)
            .status()
            .expect("cc")
            .success());
        selfhosted = String::from_utf8_lossy(
            &Command::new(&native)
                .arg(env!("CARGO_BIN_EXE_burxt"))
                .arg(scratch.join("work2"))
                .current_dir(root)
                .output()
                .expect("the self-hosted runner")
                .stdout,
        )
        .to_string();
    }

    let _ = fs::remove_dir_all(&scratch);
    assert!(
        said.contains("all green"),
        "the Burxt runner disagreed with this one:\n{}",
        said
    );
    if !selfhosted.is_empty() {
        assert!(
            selfhosted.contains("all green"),
            "the runner compiled BY BURXT disagreed:\n{}",
            selfhosted
        );
    }
    assert!(
        said.contains(&format!("ran {}, passed {}, failed 0", fixtures, fixtures)),
        "the Burxt runner checked a different set — expected {} fixtures:\n{}",
        fixtures,
        said
    );
}

/// Modules: two files, one program, and the six rules from spec/M6-MODULES.md that a
/// reader would want checked. The interesting one is the third — a diagnostic inside a
/// used module must name THAT module and its own line number, not an offset into the
/// buffer the compiler concatenated, which the programmer never saw.
#[test]
fn modules_compile_as_one_program_and_report_per_file() {
    let scratch = scratch_dir("modules");
    fs::create_dir_all(&scratch).unwrap();
    let write = |name: &str, text: &str| {
        fs::write(scratch.join(name), text).unwrap();
        scratch.join(name)
    };

    // 1 + 2: a struct and a function declared in one file, used in another.
    write(
        "lexer.bx",
        "record Tok { kind: Int, start: Int }\nfunction scan(text: String) -> Int { return len(text); }\n",
    );
    let main = write(
        "main.bx",
        "use \"lexer.bx\";\nregion r {\n  let t: Tok = Tok { kind: 7, start: 0 };\n           print(t.kind);\n  print(scan(\"hello\"));\n}\n",
    );
    let exe = scratch.join("prog");
    let built = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("run")
        .arg(&main)
        .arg("-o")
        .arg(&exe)
        .current_dir(&scratch)
        .output()
        .expect("burxt");
    assert!(
        built.status.success(),
        "a two-file program did not compile:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&built.stdout), "7\n5\n");

    // 3: an error in the used file names the used file, at its own line.
    write(
        "bad.bx",
        "// a module with a mistake\nfunction broken(a: Decimal<2>, b: Decimal<4>) -> Decimal<2> {\n           return a + b;\n}\n",
    );
    let uses_bad = write("uses_bad.bx", "use \"bad.bx\";\nprint(1);\n");
    let complained = burxt("check", &uses_bad, &scratch);
    let said = String::from_utf8_lossy(&complained.stderr).to_string();
    assert!(
        said.contains("bad.bx:3:") && said.contains("scales must match"),
        "an error in a module must name the module and its line:\n{}",
        said
    );

    // 4 + 5: a file used twice is compiled once, and two files may use each other.
    write("a.bx", "use \"b.bx\";\nfunction from_a() -> Int { return from_b() + 1; }\n");
    write("b.bx", "use \"a.bx\";\nfunction from_b() -> Int { return 41; }\n");
    let cycle = write("cycle.bx", "use \"a.bx\";\nuse \"b.bx\";\nprint(from_a());\n");
    let ran = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("run")
        .arg(&cycle)
        .arg("-o")
        .arg(scratch.join("cyc"))
        .current_dir(&scratch)
        .output()
        .expect("burxt");
    assert!(
        ran.status.success(),
        "mutual use should compile — declarations are collected before bodies:\n{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "42\n");

    // 6: a module may not hold statements.
    write("effects.bx", "print(\"I run when used\");\nfunction helper() -> Int { return 1; }\n");
    let uses_effects = write("uses_effects.bx", "use \"effects.bx\";\nprint(helper());\n");
    let refused = burxt("check", &uses_effects, &scratch);
    let why = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(!refused.status.success(), "a module with a statement must be refused");
    assert!(
        why.contains("declarations, not statements"),
        "and refused for that reason:\n{}",
        why
    );

    // 7: a missing file names who asked for it.
    let missing = write("missing.bx", "use \"nowhere.bx\";\nprint(1);\n");
    let gone = burxt("check", &missing, &scratch);
    let text = String::from_utf8_lossy(&gone.stderr).to_string();
    assert!(!gone.status.success());
    assert!(
        text.contains("nowhere.bx") && text.contains("missing.bx"),
        "a missing module must name itself and its user:\n{}",
        text
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// The standard library compiles, and does what it says. Written in Burxt from the same
/// builtins any program has — so this test is really asking whether `lib/` is *usable*,
/// which is the only interesting question about a standard library.
///
/// It also covers the two rules that only appear once modules and a library exist
/// together: an `extern fn` may be declared by two modules if the signatures match, and
/// the emitted module must declare that symbol once.
#[test]
fn the_standard_library_compiles_and_works() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("stdlib");
    fs::create_dir_all(&scratch).unwrap();

    for module in ["string.bx", "files.bx", "os.bx"] {
        let out = burxt("check", &root.join("lib").join(module), &scratch);
        assert!(
            out.status.success(),
            "lib/{} does not compile:\n{}",
            module,
            String::from_utf8_lossy(&out.stdout)
        );
    }

    // `fs.bx` and `os.bx` both wrap `system`, so using both exercises the duplicate-extern
    // rule end to end — typechecker, codegen and linker.
    let lib = root.join("lib");
    let program = scratch.join("uses_lib.bx");
    fs::write(
        &program,
        format!(
            "use \"{0}/string.bx\";\nuse \"{0}/files.bx\";\nuse \"{0}/os.bx\";\n\
             region r {{\n  print(string_find(\"hello, modules\", \"modules\"));\n               print(string_trim(\"   padded   \"));\n  print(string_to_int(\"-42\"));\n               print(string_join(string_split(\"a,b,c\", 44), \" | \"));\n               let wrote: Int = file_write(\"{1}/demo.txt\", \"first\\n\");\n               let more: Int = file_append(\"{1}/demo.txt\", \"second\\n\");\n               print(len(file_read(\"{1}/demo.txt\")));\n               print(file_exists(\"{1}/demo.txt\"));\n               print(len(file_list_directory(\"{0}\")) >= 3);\n               print(os_run(\"true\"));\n  print(string_trim(os_capture(\"echo captured\")));\n}}\n",
            lib.display(),
            scratch.display()
        ),
    )
    .unwrap();

    let exe = scratch.join("prog");
    let ran = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("run")
        .arg(&program)
        .arg("-o")
        .arg(&exe)
        .current_dir(&scratch)
        .output()
        .expect("burxt");
    let printed = String::from_utf8_lossy(&ran.stdout).to_string();
    let complained = String::from_utf8_lossy(&ran.stderr).to_string();
    assert!(ran.status.success(), "the library program failed:\n{}", complained);
    assert_eq!(
        printed,
        "7\npadded\n-42\na | b | c\n13\ntrue\ntrue\n0\ncaptured\n",
        "the library answered differently than expected"
    );

    // `Option<T>` and `Result<T, E>` are LIBRARY types — four lines of Burxt each, with no
    // compiler support beyond generics. That is the test M7 set for whether the generics are
    // real, so it is checked rather than taken on trust.
    let absence = scratch.join("uses_option.bx");
    fs::write(
        &absence,
        format!(
            "use \"{0}/option.bx\";\nuse \"{0}/result.bx\";\n\
             function find(xs: [Int], want: Int) -> Option<Int> {{\n  let mutable i = 0;\n  \
             for x in xs {{\n    if x == want {{ return Option.Some(i); }}\n    \
             i += 1;\n  }}\n  return Option.None;\n}}\n\
             function divide(a: Int, b: Int) -> Result<Int, String> {{\n  \
             if b == 0 {{ return Result.Error(\"division by zero\"); }}\n  \
             return Result.Ok(divide_toward_zero(a, b));\n}}\n\
             region r {{\n  let mutable xs: [Int] = [];\n  let a = push(xs, 5);\n  \
             let b = push(xs, 9);\n  print(option_or(find(xs, 9), 0 - 1));\n  \
             print(option_or(find(xs, 7), 0 - 1));\n  \
             print(option_is_none(find(xs, 7)));\n  \
             match divide(10, 2) {{\n    Ok(n) => {{ print(n); }}\n    \
             Error(why) => {{ print(why); }}\n  }}\n  \
             match divide(1, 0) {{\n    Ok(n) => {{ print(n); }}\n    \
             Error(why) => {{ print(why); }}\n  }}\n  \
             let words: Option<String> = Option.Some(\"here\");\n  \
             print(option_or(words, \"absent\"));\n}}\n",
            lib.display()
        ),
    )
    .unwrap();
    let absent = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("run")
        .arg(&absence)
        .arg("-o")
        .arg(scratch.join("uses_option"))
        .current_dir(&scratch)
        .output()
        .expect("burxt run");
    let said = String::from_utf8_lossy(&absent.stdout).to_string();
    let why = String::from_utf8_lossy(&absent.stderr).to_string();
    let _ = fs::remove_dir_all(&scratch);
    assert!(absent.status.success(), "the absence library failed:\n{}\n{}", said, why);
    let want = "1\n-1\ntrue\n5\ndivision by zero\nhere\n";
    assert!(
        said.ends_with(want),
        "Option/Result printed {:?}, expected it to end with {:?}",
        said,
        want
    );
}

/// Distribution: a tarball someone can unpack and use with no Rust, no cargo and no LLVM.
///
/// The binary statically links LLVM — which is why it is 14 MB compressed — so the only
/// thing it needs from a machine is a C compiler for the link step. This test builds the
/// tarball, unpacks it somewhere else, and compiles a program that uses the packaged
/// standard library, with the LLVM environment variable removed to prove it is not needed.
#[test]
#[ignore = "builds a release binary; run with --ignored when packaging"]
fn the_release_tarball_works_without_rust_or_llvm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("release");
    fs::create_dir_all(&scratch).unwrap();

    let built = Command::new("sh")
        .arg("scripts/release.sh")
        .current_dir(root)
        .output()
        .expect("scripts/release.sh");
    assert!(
        built.status.success(),
        "the release script failed:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let tarball = fs::read_dir(root.join("dist"))
        .expect("dist/")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with(".tar.gz"))
        .expect("a tarball in dist/");

    assert!(Command::new("tar")
        .arg("xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&scratch)
        .status()
        .expect("tar")
        .success());
    let unpacked = fs::read_dir(&scratch)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("the unpacked directory");

    let program = scratch.join("hello.bx");
    fs::write(
        &program,
        format!(
            "use \"{}/lib/string.bx\";\nregion r {{\n  print(string_trim(\"  packaged  \"));\n               let price: Decimal<2> = 19.99;\n  print(price * 3);\n}}\n",
            unpacked.display()
        ),
    )
    .unwrap();

    // Without LLVM_SYS_181_PREFIX: if the binary needed LLVM on the machine, this is where
    // it would say so.
    let ran = Command::new(unpacked.join("burxt"))
        .arg("run")
        .arg(&program)
        .arg("-o")
        .arg(scratch.join("prog"))
        .env_remove("LLVM_SYS_181_PREFIX")
        .output()
        .expect("the packaged burxt");
    let printed = String::from_utf8_lossy(&ran.stdout).to_string();
    let said = String::from_utf8_lossy(&ran.stderr).to_string();
    let _ = fs::remove_dir_all(&scratch);
    assert!(ran.status.success(), "the packaged compiler failed:\n{}", said);
    assert!(
        printed.contains("packaged") && printed.contains("59.97"),
        "the packaged compiler and library answered:\n{}",
        printed
    );
}

/// M9. Reading a file one byte at a time must not cost the square of the file's size.
///
/// The fixture is a small program followed by 1.5 MB of comments: the same tokens and the same
/// nodes either way, so every extra second is spent on bytes that mean nothing. Before v0.0.90
/// it took **28 seconds**; it takes 1.3 now. The 8-second budget sits between them with room on
/// both sides, so it flags the regression on a slow machine and flaps on none.
///
/// The second number is `spec/M9-PERFORMANCE.md` §6.1 written down: a self-compile inside 20
/// seconds. It was 190 seconds before the fix and 1.2 after.
///
/// What the fix was, since a threshold on its own teaches nobody: `byte_at` bounds-checks
/// against the string's length, a Burxt String is NUL-terminated, so the length is a `strlen`.
/// Stage-0 hand-wrote that scan instead of calling libc's, which meant LLVM could not prove it
/// terminated and so never hoisted it out of a loop — and stage-0 ran no IR pipeline to hoist
/// with. The check still happens; it is computed once per loop rather than once per byte.
#[test]
fn the_compiler_compiles_itself_without_going_quadratic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("m9");
    fs::create_dir_all(&scratch).unwrap();
    let stage1 = scratch.join("stage1");
    assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .expect("burxt")
        .success());

    let program: String = (0..60)
        .map(|n| {
            format!(
                "function work_{}(a: Int, b: Int) -> Int {{\n    let c: Int = a * b + a - b;\n    \
                 if c > 100 {{ return c - 1; }}\n    return c + 1;\n}}\n",
                n
            )
        })
        .collect();
    let filler: String = std::iter::repeat(format!("// {}\n", "x".repeat(80)))
        .take(18_000)
        .collect();
    let padded = scratch.join("padded.bx");
    fs::write(&padded, format!("{}{}", program, filler)).unwrap();

    let started = std::time::Instant::now();
    let ran = Command::new(&stage1).arg(&padded).output().expect("stage1");
    let on_comments = started.elapsed();
    assert!(
        String::from_utf8_lossy(&ran.stdout).contains("type errors: 0"),
        "stage-1 did not accept the padded program:\n{}",
        String::from_utf8_lossy(&ran.stdout)
    );
    assert!(
        on_comments < std::time::Duration::from_secs(8),
        "1.5 MB of comments took {:?}; the budget is 8 s (28 s before v0.0.90, 1.3 s after). \
         Reading bytes has gone quadratic again — see spec/M9-PERFORMANCE.md",
        on_comments
    );

    let started = std::time::Instant::now();
    let emitted = Command::new(&stage1)
        .arg(root.join("examples/stage1.bx"))
        .arg(scratch.join("self.ll"))
        .output()
        .expect("stage1 on its own source");
    let self_compile = started.elapsed();
    let said = String::from_utf8_lossy(&emitted.stdout).to_string();
    // DECLARATION COUNT, which is a different axis from bytes and had its own quadratic until
    // v0.0.117: declaring a function looked it up first, to refuse a duplicate, so declaring n of
    // them scanned a growing table n times. 3200 declarations took 5.52 s; with a hash index over
    // the name spans it takes 0.33 s.
    //
    // A ratio against the same compiler on a quarter of the input, so it does not depend on the
    // machine.
    //
    // **The bar is 6x, and it got there the way the comment above it said it would.** The history,
    // because a ratchet whose number nobody can account for is a number nobody will dare move:
    //
    //   50x  — a scan of every declared function per declaration, plus the String quadratic
    //   25x  — v0.0.117 indexed the declaration scan; the remaining ~16x was the String quadratic,
    //          and the bar was deliberately set above what the fix could reach rather than
    //          asserting a claim its subject did not make
    //   6x   — v0.0.121 gave a String an O(1) length, and the measured ratio is **3.4x**
    //
    // Linear is ~4x for 4x the input. 6x leaves room for constant-factor drift and nothing else.
    {
        let mut wide = String::new();
        for i in 0..3200 {
            wide.push_str(&format!("function f{}(x: Int) -> Int {{ return x + {}; }}\n", i, i));
        }
        wide.push_str("region r {\n  print(f0(1));\n}\n");
        let quarter: String = wide.lines().take(800).collect::<Vec<_>>().join("\n")
            + "\nregion r {\n  print(f0(1));\n}\n";

        let big = scratch.join("wide.bx");
        let small = scratch.join("narrow.bx");
        fs::write(&big, &wide).unwrap();
        fs::write(&small, &quarter).unwrap();

        let time_of = |path: &PathBuf| {
            let started = std::time::Instant::now();
            let ran = Command::new(&stage1).arg(path).output().expect("stage1");
            assert!(
                String::from_utf8_lossy(&ran.stdout).contains("type errors: 0"),
                "stage-1 did not accept a program of plain declarations:\n{}",
                String::from_utf8_lossy(&ran.stdout)
            );
            started.elapsed().as_secs_f64()
        };
        // Warmed first: the first run pays for reading the binary off disk, and that lands
        // entirely in whichever measurement goes first.
        let _ = time_of(&small);
        let narrow = time_of(&small).max(0.001);
        let broad = time_of(&big);
        let ratio = broad / narrow;
        eprintln!(
            "3200 declarations took {:.3} s, 800 took {:.3} s — {:.1}x for 4x the input",
            broad, narrow, ratio
        );
        assert!(
            ratio < 6.0,
            "declaring functions costs {:.1}x for 4x the declarations ({:.3} s vs {:.3} s). \
             Linear is ~4x and it measured 3.4x at v0.0.121. Above 6x means either the name-span \
             index in check.bx is gone or a String has stopped carrying its length.",
            ratio,
            broad,
            narrow
        );
    }

    // And a ceiling on MEMORY, which had no test until v0.0.110 and drifted 196 MB -> 239 MB
    // across the generics work without anybody noticing. The number that matters is not the
    // last measurement — it is the 1 GB region the compiler reserves, which it came within a
    // hair of exhausting before v0.0.90. So the ceiling is 400 MB: high enough that ordinary
    // growth does not trip it, low enough that a return to the wall fails here first.
    //
    // Peak RSS, not allocation count, because the region touches its pages and the pages are
    // what run out. Skipped rather than failed when /usr/bin/time is absent, since a test that
    // cannot measure must not claim a verdict.
    if std::path::Path::new("/usr/bin/time").exists() {
        let measured = Command::new("/usr/bin/time")
            .arg("-f")
            .arg("%M")
            .arg(&stage1)
            .arg(root.join("examples/stage1.bx"))
            .arg(scratch.join("self-memory.ll"))
            .output()
            .expect("time on stage1");
        let reported = String::from_utf8_lossy(&measured.stderr);
        let kb: u64 = reported
            .lines()
            .last()
            .unwrap_or("0")
            .trim()
            .parse()
            .unwrap_or(0);
        assert!(kb > 0, "could not read peak RSS from:\n{}", reported);
        eprintln!("the compiler's peak RSS on its own source: {} MB", kb / 1024);
        assert!(
            kb < 400 * 1024,
            "the compiler's peak RSS on its own source is {} MB; the ceiling is 400 MB, and \
             the region it reserves is 1 GB (196 MB at v0.0.90, 239 MB at v0.0.110, 335 MB at \
             v0.0.121 — the eight-byte length header on every String, and larger emitted IR)",
            kb / 1024
        );
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(said.contains("bytes of IR"), "stage-1 did not emit its own source:\n{}", said);
    assert!(
        self_compile < std::time::Duration::from_secs(20),
        "the compiler took {:?} on its own source; the budget is 20 s (190 s before v0.0.90, \
         1.2 s after)",
        self_compile
    );

}

// `generics_monomorphise_and_run` lived here until v0.0.111, with nine cases and a rationale
// that said stage-1 "does not read generics yet" — so a pass fixture would have asserted
// something untrue about the other compiler. That is no longer true, and a test whose reason for
// existing has expired is worse than no test: it looks like coverage.
//
// Its nine grounds are now covered by four fixtures in tests/pass/ — generics_functions (a `[T]`
// element, a bound, a trait bound, a generic calling a generic), generics_types (two type
// parameters, Option, Result), generics_layout (separate layouts, a generic inside a generic) and
// generics_methods (a method on a generic type) — and a fixture there is held against BOTH
// compilers end to end by the pass-suite sweep, which the inline test never did. Strictly more
// coverage in strictly less code, so the weaker one is gone rather than kept for company.


/// The editor grammar must actually TOKENIZE the language, not merely contain its words.
///
/// `editor_grammar_knows_every_keyword_the_compiler_does` checks the vocabulary. It passed
/// happily while `function (self) price()` — the receiver shorthand shipped in v0.0.95 —
/// highlighted as nothing at all, because the method pattern still demanded `self: Type`.
/// A keyword list is not a grammar.
///
/// So: take every declaration line out of the real examples and require that some pattern in
/// the grammar's `declarations` set matches it from column zero. Anything the examples can
/// say, the editor has to colour.
#[test]
fn editor_grammar_highlights_every_declaration_the_examples_write() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let grammar =
        fs::read_to_string(root.join("editors/vscode/syntaxes/burxt.tmLanguage.json")).unwrap();

    // The `match` regexes inside the "declarations" repository entry, pulled out textually —
    // the same deliberately-simple approach the keyword test uses, so this needs no crates.
    let declarations = grammar
        .split("\"declarations\"")
        .nth(1)
        .expect("the grammar has a `declarations` repository entry");
    let end = declarations.find("\n    },").unwrap_or(declarations.len());
    let patterns: Vec<String> = declarations[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("\"match\": \""))
        .map(|l| l.trim_end_matches("\",").trim_end_matches('"').replace("\\\\", "\\"))
        .collect();
    assert!(
        patterns.len() >= 5,
        "failed to read the declaration patterns out of the grammar (found {:?})",
        patterns
    );

    // A declaration line is one that opens a function, method, record, enum, trait, impl or
    // region. Contract clauses and bodies are not declarations and are not checked here.
    let opens = ["function ", "external function ", "record ", "enum ", "trait ", "interface ",
                 "implement ", "region "];
    let mut sources: Vec<PathBuf> = Vec::new();
    for dir in ["examples", "lib"] {
        collect_bx(&root.join(dir), &mut sources);
    }
    assert!(sources.len() > 15, "expected to sweep the examples, got {}", sources.len());

    let mut unmatched: Vec<String> = Vec::new();
    for source in &sources {
        let text = fs::read_to_string(source).unwrap();
        for (n, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if !opens.iter().any(|o| trimmed.starts_with(o)) {
                continue;
            }
            if !patterns.iter().any(|p| matches_at_start(p, trimmed)) {
                unmatched.push(format!(
                    "{}:{}: {}",
                    source.strip_prefix(root).unwrap_or(source).display(),
                    n + 1,
                    trimmed.chars().take(72).collect::<String>()
                ));
            }
        }
    }
    assert!(
        unmatched.is_empty(),
        "the editor grammar does not highlight these declarations — a reader would see them \
         uncoloured. Add or widen a pattern in editors/vscode/syntaxes/burxt.tmLanguage.json:\n{}",
        unmatched.join("\n")
    );
}

/// Every `.bx` file under a directory, recursively.
fn collect_bx(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            // `negative/` holds programs that are deliberately wrong; they still have to
            // highlight, so they are included.
            collect_bx(&path, out);
        } else if path.extension().is_some_and(|x| x == "bx") {
            out.push(path);
        }
    }
}

/// Does this TextMate pattern match `line` starting at column zero?
///
/// A deliberately small regex subset — enough for the shapes the declaration patterns use
/// (`\b`, `\s*`, `\s+`, literal alternations in groups, and `[...]` classes with `*`/`+`) —
/// so the test needs no regex crate and stays readable next to the grammar it checks.
fn matches_at_start(pattern: &str, line: &str) -> bool {
    fn walk(p: &[u8], t: &[u8]) -> bool {
        if p.is_empty() {
            return true;
        }
        // \b at the start of a declaration pattern is always satisfied at column zero.
        if p.starts_with(b"\\b") {
            return walk(&p[2..], t);
        }
        if p.starts_with(b"\\s") {
            let quant = p.get(2).copied();
            let least = if quant == Some(b'+') { 1 } else { 0 };
            let rest = if matches!(quant, Some(b'*') | Some(b'+')) { &p[3..] } else { &p[2..] };
            let mut seen = 0;
            let mut i = 0;
            while i < t.len() && (t[i] as char).is_whitespace() {
                i += 1;
                seen += 1;
                if walk(rest, &t[i..]) && seen >= least {
                    return true;
                }
            }
            return seen >= least && walk(rest, &t[i..]);
        }
        if p[0] == b'(' {
            // A group: try each alternative, honouring a trailing `?`.
            let close = balanced(p).unwrap_or(p.len());
            let inner = &p[1..close];
            let mut after = close + 1;
            let optional = p.get(after).copied() == Some(b'?');
            if optional {
                after += 1;
            }
            for alt in split_alts(inner) {
                let mut joined = alt.to_vec();
                joined.extend_from_slice(&p[after..]);
                if walk(&joined, t) {
                    return true;
                }
            }
            return optional && walk(&p[after..], t);
        }
        if p[0] == b'[' {
            let close = p.iter().position(|&c| c == b']').unwrap_or(p.len() - 1);
            let class = &p[1..close];
            let quant = p.get(close + 1).copied();
            let rest_at = if matches!(quant, Some(b'*') | Some(b'+')) { close + 2 } else { close + 1 };
            let least = if quant == Some(b'+') { 1 } else if quant == Some(b'*') { 0 } else { 1 };
            let one = |c: u8| in_class(class, c);
            if quant.is_none() {
                return !t.is_empty() && one(t[0]) && walk(&p[rest_at..], &t[1..]);
            }
            let mut i = 0;
            while i < t.len() && one(t[i]) {
                i += 1;
            }
            // Greedy, then back off — enough for these patterns.
            let mut take = i;
            loop {
                if take >= least && walk(&p[rest_at..], &t[take..]) {
                    return true;
                }
                if take == 0 {
                    return false;
                }
                take -= 1;
            }
        }
        if p[0] == b'\\' {
            return t.first() == p.get(1) && walk(&p[2..], &t[1..]);
        }
        !t.is_empty() && t[0] == p[0] && walk(&p[1..], &t[1..])
    }

    fn balanced(p: &[u8]) -> Option<usize> {
        let mut depth = 0;
        for (i, &c) in p.iter().enumerate() {
            match c {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn split_alts(inner: &[u8]) -> Vec<Vec<u8>> {
        let mut out = vec![Vec::new()];
        let mut depth = 0;
        for &c in inner {
            match c {
                b'(' => {
                    depth += 1;
                    out.last_mut().unwrap().push(c);
                }
                b')' => {
                    depth -= 1;
                    out.last_mut().unwrap().push(c);
                }
                b'|' if depth == 0 => out.push(Vec::new()),
                _ => out.last_mut().unwrap().push(c),
            }
        }
        out
    }

    fn in_class(class: &[u8], c: u8) -> bool {
        let mut i = 0;
        while i < class.len() {
            // An escape INSIDE a class. Without this, `[A-Za-z0-9_,\s]` read the backslash and
            // the `s` as two literal bytes, so a space never matched and
            // `function (self: Map<K, V>) probe(...)` was reported as un-highlighted while the
            // grammar highlighted it perfectly well. `Pair<T>` has no space, which is why the
            // hole survived until a generic with two parameters was written.
            //
            // A test that cannot read its own input reports a fault in the wrong place, and that
            // is worse than a test that fails: it sends the reader to fix something that is right.
            if class[i] == b'\\' && i + 1 < class.len() {
                let matched = match class[i + 1] {
                    b's' => (c as char).is_whitespace(),
                    b'w' => (c as char).is_alphanumeric() || c == b'_',
                    b'd' => c.is_ascii_digit(),
                    other => other == c,
                };
                if matched {
                    return true;
                }
                i += 2;
                continue;
            }
            if i + 2 < class.len() && class[i + 1] == b'-' {
                if c >= class[i] && c <= class[i + 2] {
                    return true;
                }
                i += 3;
            } else {
                if class[i] == c {
                    return true;
                }
                i += 1;
            }
        }
        false
    }

    walk(pattern.as_bytes(), line.as_bytes())
}

/// A packaged extension must not be older than the grammar it packages.
///
/// This is the bug that actually reached the user: the keyword rename landed in the repo, and
/// the editor kept colouring the language it knew yesterday — because `burxt-0.1.3.vsix` had
/// been built before the rename and was still what VS Code had installed. Nothing in the repo
/// noticed, because nothing was looking.
#[test]
fn the_packaged_extension_matches_the_grammar_in_the_repository() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("editors/vscode");
    let grammar = fs::read_to_string(dir.join("syntaxes/burxt.tmLanguage.json")).unwrap();

    let packages: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "vsix"))
        .collect();
    if packages.is_empty() {
        // Nothing packaged yet is fine — an unbuilt package cannot be stale.
        return;
    }

    // A .vsix is a ZIP. Rather than depend on a zip crate, read the grammar back out with
    // Python, which the packer already uses and every machine running this suite has.
    for package in &packages {
        let read = Command::new("python3")
            .arg("-c")
            .arg(
                "import sys, zipfile\n\
                 z = zipfile.ZipFile(sys.argv[1])\n\
                 for n in z.namelist():\n\
                 \x20   if n.endswith('tmLanguage.json'):\n\
                 \x20       sys.stdout.write(z.read(n).decode()); break\n",
            )
            .arg(package)
            .output()
            .expect("python3");
        assert!(read.status.success(), "could not read {}", package.display());
        let packaged = String::from_utf8_lossy(&read.stdout).to_string();
        assert!(
            !packaged.is_empty(),
            "{} contains no grammar at all",
            package.display()
        );
        assert_eq!(
            packaged.trim(),
            grammar.trim(),
            "{} was packaged from an older grammar — the editor would highlight a language \
             this repository no longer has. Re-run `python3 editors/vscode/pack.py`.",
            package.file_name().unwrap().to_string_lossy()
        );
    }
}

/// The editor must check the PROGRAM, not the file.
///
/// `examples/burxt/check.bx` is one of five modules `examples/stage1.bx` assembles. Checked on
/// its own it reports every type declared in a sibling as unknown — so opening the compiler in
/// an editor showed five files of squiggles that were not mistakes. And `stage1.bx` itself
/// reported a parse error on its own `use` lines, because the language server never resolved
/// imports at all.
///
/// Both are the same bug: a file is not always a program.
#[test]
fn the_language_server_checks_the_program_a_file_belongs_to() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // The Burxt files that MUST be clean in an editor: every real example, the standard
    // library, and each module of the compiler. `examples/negative/` is excluded on purpose —
    // those are meant to be wrong, and a squiggle there is the point.
    let mut sources: Vec<PathBuf> = Vec::new();
    for dir in ["examples", "lib"] {
        collect_bx(&root.join(dir), &mut sources);
    }
    sources.retain(|p| !p.components().any(|c| c.as_os_str() == "negative"));
    sources.push(root.join("tests/runner.bx"));
    assert!(sources.len() > 15, "expected to sweep the examples, got {}", sources.len());

    let mut noisy = Vec::new();
    for source in &sources {
        let text = fs::read_to_string(source).unwrap();
        let uri = format!("file://{}", source.display());
        let mut request = String::new();
        let add = |body: String, request: &mut String| {
            request.push_str(&format!("Content-Length: {}\r\n\r\n{}", body.len(), body));
        };
        add(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string(), &mut request);
        add(
            format!(
                r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":{},"languageId":"burxt","version":1,"text":{}}}}}}}"#,
                json_string(&uri),
                json_string(&text)
            ),
            &mut request,
        );

        let mut child = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("lsp")
            .current_dir(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("burxt lsp");
        use std::io::Write as _;
        child.stdin.as_mut().unwrap().write_all(request.as_bytes()).unwrap();
        drop(child.stdin.take());
        let out = child.wait_with_output().expect("lsp output");
        let said = String::from_utf8_lossy(&out.stdout).to_string();

        let published = said
            .split("publishDiagnostics")
            .nth(1)
            .unwrap_or_else(|| panic!("no diagnostics for {}", source.display()));
        // Empty is `"diagnostics":[]`; anything else means the editor drew something.
        if !published.contains("\"diagnostics\":[]") {
            let shown: String = published.chars().take(220).collect();
            noisy.push(format!("{}: {}", source.strip_prefix(root).unwrap_or(source).display(), shown));
        }
    }
    assert!(
        noisy.is_empty(),
        "the language server reported problems in files that compile — a file is not always a \
         program, and these belong to one:\n{}",
        noisy.join("\n\n")
    );
}

/// A JSON string literal, escaped enough for the LSP requests above.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Stage-1's front end handles generics; only its backend does not.
///
/// It binds type parameters at the call site and from an application's arguments, resolves them
/// lazily as comparison recurses, resolves a field's or payload's type DEEPLY where it is read,
/// and enforces `Ordered`/`Equatable`/trait bounds. What remains is layout — `Option<Int>` needs
/// its own tag-and-payload sizing, one copy per argument list — so the refusal lives in the
/// emitter, and the backend ratchet covers it as a floor.
///
/// Before any guard existed, stage-1 parsed a generic, walked a type-parameter node, looked its
/// name up as a record, got -1 and indexed an array with it: exit 70. That is why this test also
/// asserts it does not die.
///
/// A compiler that half-understands a construct answers differently from the other one, and the
/// differential test exists to stop exactly that.
#[test]
fn the_burxt_compiler_reads_and_emits_every_generic_form() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("stage1-generics");
    fs::create_dir_all(&scratch).unwrap();
    let stage1 = scratch.join("stage1");
    assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .expect("burxt")
        .success());

    // One of each generic form in one program — function, bounded function, generic record,
    // generic enum, method on a generic type — so no single form can regress unnoticed. Every
    // one of them must now PARSE, CHECK and EMIT: this test asserted a refusal until v0.0.111,
    // and the refusal is gone.
    let program = scratch.join("generic.bx");
    fs::write(
        &program,
        "enum Option<T> { None, Some(T) }\n\
         record Stack<T> { items: [T] }\n\
         function identity<T>(x: T) -> T { return x; }\n\
         function largest<T: Ordered>(a: T, b: T) -> T { if a > b { return a; } return b; }\n\
         function (self: Stack<T>) count() -> Int { return len(self.items); }\n\
         region r {\n  print(identity(3));\n  print(largest(3, 9));\n  \
         let found: Option<Int> = Option.None;\n  print(1);\n}\n",
    )
    .unwrap();

    let ran = Command::new(&stage1)
        .arg(&program)
        .arg(scratch.join("generic.ll"))
        .current_dir(&scratch)
        .output()
        .expect("stage1");
    let said = String::from_utf8_lossy(&ran.stdout).to_string();
    let complained = String::from_utf8_lossy(&ran.stderr).to_string();

    // It must not die. A runtime failure exits 70; a refusal is an ordinary run.
    assert!(
        ran.status.success(),
        "stage-1 died on a generic program instead of refusing it:\n{}\n{}",
        said,
        complained
    );
    // The PARSER must be complete: every form above read without complaint.
    assert!(
        said.contains("parse errors: 0"),
        "stage-1 could not parse a generic form the Rust parser accepts:\n{}",
        said
    );
    // And the CHECKER must say so rather than pretend.
    assert!(
        said.contains("type errors: 0"),
        "stage-1's front end must accept generics — it binds type parameters at the call \
         site, resolves them lazily, and resolves a field or payload deeply where it is \
         read:\n{}",
        said
    );
    // And the BACKEND must emit them, which is what changed in v0.0.111.
    assert!(
        said.contains("bytes of IR"),
        "stage-1's backend must emit every generic form:\n{}\n{}",
        said,
        complained
    );
    // A call to an UNMANGLED generic is what a missed suffix looks like: the module links
    // against a function that was never defined, which is a link error rather than a wrong
    // answer. Cheaper to grep for than to discover at `cc` time.
    let ir = fs::read_to_string(scratch.join("generic.ll")).expect("the IR");
    for named in ["identity", "largest"] {
        assert!(
            !ir.contains(&format!("call i64 @{}(", named)),
            "a call to the unmangled generic `{}` survived: every call must name the copy it \
             is calling, or the module will not link",
            named
        );
    }
    let _ = fs::remove_dir_all(&scratch);
}

/// **One word per concept.** A lookup is `find_<thing>`, the array it searches is `<thing>s`, the
/// record it holds is `<Thing>`, and its counter is `find_<thing>_calls`.
///
/// This convention already existed and was already followed by `method`, `slot`, `instance` and
/// `type` — nobody had written it down, so two families drifted and stayed drifted:
///
///   `find_sym` searched `self.syms`, which held `Binding` records, and incremented
///   `find_binding_calls`. FOUR names for one concept, in one function, three of them visible in a
///   five-line body. And `find_fun` searched `self.funs` while its counter said `function`.
///
/// Both were fixed in v0.0.122. This test is why they cannot come back — the audit that found them
/// was a one-off, and a one-off finds a thing once.
///
/// It also refuses clipped names on FIELDS, which cross files and so mislead furthest. Short-lived
/// locals are left alone deliberately: inside one function the declaration is on screen with the
/// use, and a rule that reaches that far would be a rule people route around.
#[test]
fn one_word_per_concept_in_the_burxt_compiler() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources: Vec<PathBuf> = Vec::new();
    collect_bx(&root.join("examples/burxt"), &mut sources);
    sources.push(root.join("examples/stage1.bx"));
    let mut text = String::new();
    for p in &sources {
        text.push_str(&fs::read_to_string(p).unwrap());
        text.push('\n');
    }
    // Comments and string literals are prose and emitted IR, not identifiers.
    let mut code = String::new();
    for line in text.lines() {
        let line = match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        };
        let mut in_string = false;
        let mut prev = ' ';
        for c in line.chars() {
            if c == '"' && prev != '\\' {
                in_string = !in_string;
                prev = c;
                continue;
            }
            if !in_string {
                code.push(c);
            }
            prev = c;
        }
        code.push('\n');
    }

    // Every `find_<thing>` must have a `self.<thing>s` array, and the singular must not also
    // appear as a different spelling of the same word.
    let mut problems: Vec<String> = Vec::new();
    for family in ["binding", "function", "type", "method", "slot", "instance"] {
        let lookup = format!("find_{}(", family);
        if !code.contains(&lookup) {
            problems.push(format!(
                "no `find_{}` — the lookup for `{}` was renamed away from the convention",
                family, family
            ));
        }
    }
    // The clipped spellings these families drifted into. Each one is a name that used to exist.
    for banned in ["find_sym", "self.syms", "self.funs", "find_fun(", "fun_buckets", "fun_chain"] {
        if code.contains(banned) {
            problems.push(format!(
                "`{}` is back. One word per concept: a lookup is find_<thing>, its array is \
                 <thing>s, its record is <Thing>",
                banned
            ));
        }
    }

    // Clipped FIELD names. Fields cross files, so an abbreviation in one costs every reader.
    let clipped = [
        "sym", "syms", "ty", "tys", "decl", "decls", "expr", "stmt", "arg", "args", "param",
        "params", "idx", "val", "var", "tmp", "buf", "cnt", "num", "msg", "err", "ret", "elem",
        "attr", "ctx", "cfg", "sig", "dest", "pos", "prev", "curr", "iter", "acc", "tok", "toks",
        "fn", "fns", "mut", "recv", "len",
    ];
    let types_bx = fs::read_to_string(root.join("examples/burxt/types.bx")).unwrap();
    for line in types_bx.lines() {
        let line = match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        };
        // A function SIGNATURE also has `name: Type` pairs, and its parameters are locals — the
        // very thing this rule deliberately does not reach. `spans_equal(src, a, a_len, b, b_len)`
        // was reported as four clipped fields before this line existed.
        if line.trim_start().starts_with("function ") {
            continue;
        }
        for piece in line.split(',') {
            let Some((name, _)) = piece.split_once(':') else { continue };
            let name = name.trim().trim_start_matches("record ");
            if !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') || name.is_empty() {
                continue;
            }
            // `receiver_length` ends in a real word; `receiver_len` does not.
            let last = name.rsplit('_').next().unwrap_or(name);
            if clipped.contains(&name) || (name.contains('_') && clipped.contains(&last)) {
                problems.push(format!(
                    "field `{}` in examples/burxt/types.bx is clipped. A field crosses files, so \
                     write the word: `length` not `len`, `parameters` not `params`, `position` \
                     not `pos`",
                    name
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "naming drifted — see spec/A7.0-NAMING.md:\n{}",
        problems.join("\n")
    );
}

/// **The site cannot claim what the compiler does not do, and cannot quietly lose a page.**
///
/// Two failures this guards, both of which have already happened once in this project:
///
///   1. Output typed by hand and then drifting. `docs/examples.md` is GENERATED by
///      `scripts/site-examples.py`, which runs every snippet through the real compiler. This test
///      regenerates it and diffs, so a change in behaviour breaks the build rather than the page.
///   2. A page nobody links to. Eleven guide pages exist; the site's index has to reach all of them,
///      the same rule `the_guide_and_examples_are_linked_and_compile` already applies to the
///      repository's own index.
#[test]
fn the_site_is_honest_and_complete() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Every guide page is reachable from the site's guide index.
    let index = fs::read_to_string(root.join("docs/guide/index.md")).expect("the site guide index");
    let mut pages = 0;
    for entry in fs::read_dir(root.join("docs/guide")).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name == "index.md" || name == "README.md" {
            continue;
        }
        pages += 1;
        // Jekyll serves `04-memory.md` as `04-memory.html`, so that is what the index must link.
        let as_page = name.replace(".md", ".html");
        assert!(
            index.contains(&as_page),
            "docs/guide/index.md does not link {} — a visitor cannot reach that page",
            as_page
        );
    }
    assert!(pages >= 11, "the site lost guide pages: {} left", pages);

    // Front matter, without which Jekyll copies the markdown verbatim and a visitor sees raw
    // asterisks instead of a page. Silent, and only visible by loading the site.
    for entry in fs::read_dir(root.join("docs/guide")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if path.file_name().unwrap() == "README.md" {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("---\n"),
            "{} has no front matter, so Jekyll will not render it — the site would show raw \
             markdown",
            path.file_name().unwrap().to_string_lossy()
        );
    }

    // Markdown inside a raw <div> needs `markdown="1"`, or kramdown passes it through untouched.
    //
    // This shipped to the live site: `## Money is not a float` and three other headings reached
    // burxt-lang.org as literal hashes, because a `<div class="wrap">` around them made kramdown
    // treat the whole block as raw HTML. The tables rendered — they had `markdown="1"` — which is
    // what made the page look mostly right and the bug easy to miss.
    //
    // Only visible by loading the site, which is why it needs a test rather than care.
    for page in ["index.md", "install/index.md", "examples/index.md", "guide/index.md"] {
        let text = match fs::read_to_string(root.join("docs").join(page)) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut open_div: Option<(usize, String)> = None;
        for (n, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("<div") {
                // Only a div that does NOT already say markdown="1" can trap anything.
                open_div = if trimmed.contains("markdown=") {
                    None
                } else {
                    Some((n + 1, trimmed.chars().take(48).collect()))
                };
                continue;
            }
            if trimmed.starts_with("</div>") {
                open_div = None;
                continue;
            }
            if let Some((at, which)) = &open_div {
                // A heading or a table row is markdown that will not survive.
                let is_markdown = trimmed.starts_with("#") || trimmed.starts_with("|");
                assert!(
                    !is_markdown,
                    "docs/{}:{} — `{}` has no markdown=\"1\", so the markdown on line {} reaches \
                     the live page as literal text. kramdown treats a raw <div> as raw HTML.",
                    page, at, which, n + 1
                );
            }
        }
    }

    // The Codespace must BUILD the extension, not assume one is lying around.
    //
    // `.gitignore` has `*.vsix`, on the sound principle that a binary in a repository is a binary
    // nobody can reproduce. So a fresh clone has no package — and the first real Codespace found
    // exactly that: the compiler ran fine and the editor had no highlighting and no diagnostics,
    // because setup.sh looked for a .vsix that git had never carried.
    //
    // `pack.py` needs only the standard library, so building it in the container is free. This
    // asserts the container does that rather than hoping.
    let setup = fs::read_to_string(root.join(".devcontainer/setup.sh")).expect("the setup script");
    assert!(
        setup.contains("pack.py"),
        ".devcontainer/setup.sh must BUILD the extension with editors/vscode/pack.py. The .vsix is \
         git-ignored, so a fresh clone has none and the editor gets no highlighting or diagnostics."
    );
    // And the compiler has to land where an extension host can find it. A PATH edited in .bashrc is
    // not inherited by one, which is why ~/.local/bin left the language server unable to start.
    assert!(
        setup.contains("/usr/local"),
        ".devcontainer/setup.sh must install the compiler somewhere already on PATH — an extension \
         host does not inherit a PATH set in .bashrc, so the language server will not start"
    );

    // Every link in the site's NAVIGATION has a page behind it.
    //
    // This is the check that was missing when the site first went live: `/examples/` and `/install/`
    // both 404'd, because Jekyll serves a bare `examples.md` at `/examples.html` and only
    // `<dir>/index.md` earns the directory URL. `/guide/` worked from the start for exactly that
    // reason, which is what made the other two look like they should.
    //
    // A 404 on a launched site is the cheapest possible bug to prevent and one of the most
    // embarrassing to ship.
    let layout = fs::read_to_string(root.join("docs/_layouts/default.html")).expect("the layout");
    for target in ["guide", "examples", "install"] {
        let link = format!("{{{{ site.baseurl }}}}/{}/", target);
        if !layout.contains(&link) {
            continue;                       // not in the nav, so nothing to serve
        }
        let page = root.join("docs").join(target).join("index.md");
        assert!(
            page.exists(),
            "the navigation links /{}/ but docs/{}/index.md does not exist — that URL will 404. \
             Jekyll only gives a directory URL to <dir>/index.md; a bare {}.md is served at \
             /{}.html",
            target, target, target, target
        );
    }

    // The generated examples page is current. Skipped rather than failed when the release binary is
    // absent, because the generator needs a compiler and a debug build is not what the site quotes.
    if !root.join("target/release/burxt").exists() {
        eprintln!("skipping the examples-page check: no release binary (cargo build --release)");
        return;
    }
    let checked = Command::new("python3")
        .arg("scripts/site-examples.py")
        .arg("--check")
        .current_dir(root)
        .output()
        .expect("the site example generator");
    assert!(
        checked.status.success(),
        "docs/examples/index.md no longer matches what the compiler does. Regenerate it:\n    \
         python3 scripts/site-examples.py\n{}{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
}

/// **Every runtime guarantee, held against the Burxt backend too.**
///
/// `tests/panic/` is the suite's record of what must FAIL at run time: a broken contract, an
/// overflow, an index out of range, a `decreases` measure that does not decrease. Until v0.0.136 it
/// was checked against stage-0 only, and the hole that left was not small — **12 of its 21
/// guarantees did not survive stage-1's backend, and one hung forever.**
///
/// Why 35 other invariants missed it, which is the part worth remembering: every contract fixture in
/// `tests/pass/` has contracts that SUCCEED, and a satisfied contract produces identical output
/// whether or not it was checked. So those fixtures prove contracts do not break working programs —
/// not that they fire. The programs where one fires exit 70, so they live here, and the pass-suite
/// sweep only ever reads `.stdout` from `tests/pass/`.
///
/// The one test that would have caught it was in the one directory that test never looked at. A gap
/// shaped exactly like a directory boundary.
///
/// A FLOOR, not an equality, because the fix is a family of runtime checks in stage-1's emitter and
/// they will land a few at a time. It may only go up.
#[test]
fn the_burxt_backend_keeps_every_runtime_guarantee() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    if !llc.exists() {
        eprintln!("skipping: {} is not installed", llc.display());
        return;
    }
    // One fixture recurses forever when its `decreases` measure is not enforced, so every run is
    // bounded. Without `timeout` the whole suite would hang rather than report.
    if Command::new("timeout").arg("1").arg("true").status().is_err() {
        eprintln!("skipping: `timeout` is not available to bound a runaway program");
        return;
    }
    let scratch = scratch_dir("panic-backend");
    fs::create_dir_all(&scratch).unwrap();
    let stage1 = scratch.join("stage1");
    assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .expect("burxt")
        .success());

    let mut kept = 0;
    let mut total = 0;
    let mut lost: Vec<String> = Vec::new();
    for entry in fs::read_dir(root.join("tests/panic")).unwrap() {
        let source = entry.unwrap().path();
        if source.extension().and_then(|e| e.to_str()) != Some("bx") {
            continue;
        }
        let name = source.file_stem().unwrap().to_string_lossy().into_owned();
        total += 1;
        let ll = scratch.join("panic.ll");
        let emitted = Command::new(&stage1).arg(&source).arg(&ll).output().expect("stage-1");
        if !String::from_utf8_lossy(&emitted.stdout).contains("bytes of IR") {
            lost.push(format!("{} (backend refused it)", name));
            continue;
        }
        let obj = scratch.join("panic.o");
        let exe = scratch.join("panic-run");
        if !Command::new(llc)
            .args(["-filetype=obj", "-relocation-model=pic"])
            .arg(&ll)
            .arg("-o")
            .arg(&obj)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            lost.push(format!("{} (its IR does not assemble)", name));
            continue;
        }
        if !Command::new("cc")
            .arg(&obj)
            .arg("-o")
            .arg(&exe)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            lost.push(format!("{} (does not link)", name));
            continue;
        }
        // Must die. Which signal or code does not matter here — the pass-suite sweep already
        // checks the MESSAGE for programs that succeed, and what this test is about is whether the
        // guarantee exists at all.
        let ran = Command::new("timeout")
            .arg("5")
            .arg(&exe)
            .current_dir(&scratch)
            .output()
            .expect("the compiled program");
        match ran.status.code() {
            Some(0) => lost.push(format!("{} (ran to completion — the check is missing)", name)),
            Some(124) => lost.push(format!("{} (never terminated)", name)),
            _ => kept += 1,
        }
    }

    let _ = fs::remove_dir_all(&scratch);
    eprintln!(
        "the Burxt backend keeps {} of {} runtime guarantees",
        kept, total
    );
    assert!(
        kept >= 11,
        "the Burxt backend keeps {} of {} runtime guarantees, was 11 of 21 at v0.0.137, and 8 when the sweep was added at v0.0.136. \
         These are lost — a program compiled by stage-1 does not enforce them:\n  {}",
        kept,
        total,
        lost.join("\n  ")
    );
}

/// Every source and documentation file must be IN version control.
///
/// `.gitignore` uses a whitelist — `/*` then re-admit — which is the right shape for keeping
/// stray build artifacts out of the root, and strictly more dangerous for new directories: the
/// failure is silent. `lib/`, `docs/` and `scripts/` were never re-admitted, so from v0.0.31
/// the standard library, the entire guide and the whole milestone log lived only on one disk.
/// `git status` stayed clean the whole time.
///
/// Three tests in this file READ those directories, so the suite would have failed on a fresh
/// clone while passing here — the worst shape a test failure can have. Found by a `git mv` of
/// a library file refusing, not by anything looking.
#[test]
fn every_source_and_document_is_in_version_control() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tracked = Command::new("git")
        .args(["ls-files"])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(tracked.status.success(), "git ls-files failed — is this a git checkout?");
    let tracked: std::collections::HashSet<String> = String::from_utf8_lossy(&tracked.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();
    assert!(tracked.len() > 100, "expected a populated index, got {}", tracked.len());

    // Everything that IS source or documentation. Build outputs and packages are excluded on
    // purpose: those are reproducible, and a binary in a repository is a binary nobody can
    // rebuild.
    let mut untracked: Vec<String> = Vec::new();
    let mut walk = vec![root.to_path_buf()];
    while let Some(dir) = walk.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // Dot-directories are skipped EXCEPT the four that carry real configuration. Skipping
            // all of them left this test blind to exactly what it exists to protect: CI, the
            // Codespace config, the LLVM prefix the build reads, and the editor settings are each a
            // file whose loss would be silent, and each was tracked only because `!/*.*` in
            // .gitignore happens to match a leading dot.
            let carries_configuration = matches!(
                name.as_str(),
                ".cargo" | ".devcontainer" | ".github" | ".vscode"
            );
            if (name.starts_with('.') && !carries_configuration)
                || matches!(name.as_str(), "target" | "dist" | "node_modules")
            {
                continue;
            }
            if path.is_dir() {
                walk.push(path);
                continue;
            }
            let interesting = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "bx" | "md" | "rs" | "toml" | "json" | "sh" | "py"));
            if !interesting {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap().to_string_lossy().to_string();
            if !tracked.contains(&relative) {
                untracked.push(relative);
            }
        }
    }
    untracked.sort();
    assert!(
        untracked.is_empty(),
        "these source or documentation files are not in version control — `.gitignore` is a \
         whitelist, so a new directory is ignored until it is re-admitted:\n{}",
        untracked.join("\n")
    );
}
