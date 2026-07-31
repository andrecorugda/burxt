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
/// adding an interface implementation could move a field, and codegen written
/// against these offsets would break.
#[test]
fn record_layout_has_no_hidden_header() {
    let scratch = scratch_dir("layout");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("layout_probe.bx");
    fs::write(
        &program,
        "class Money { amount: Decimal<2> }\n\
         class LineItem { price: Decimal<2>, qty: Int }\n\
         class Order { total: Money, items: Int, label: String }\n\
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
/// be byte-identical whether or not it is ever used as an interface object, because
/// the vtable lives OUTSIDE the value. Also checks the pay-for-what-you-use
/// rule: a program with no `dyn` emits no vtable at all.
#[test]
fn dynamic_does_not_change_layout_and_costs_nothing_unused() {
    let scratch = scratch_dir("dynamic-layout");
    fs::create_dir_all(&scratch).unwrap();

    let common = "interface Priced { function price(self) -> Decimal<2> }\n\
                  class Book { cost: Decimal<2>, pages: Int }\n\
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
        "becoming an interface object moved a field — the vtable must live outside the value"
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

    // Built-in functions, from `is_reserved_name` in the typechecker.
    //
    // This used to read `f.name == "` — a shape the typechecker had when the test was written and
    // does not have now, because those comparisons were collected into one `matches!`. So the scrape
    // found NOTHING and quietly contributed an empty list, for however many versions that refactor
    // is old. The test went on passing on its keywords alone, and `exit` was missing from the
    // grammar the whole time.
    //
    // Hence the floor below. A scrape that finds nothing must fail rather than check less: this file
    // already learned that lesson once, in the generator that skipped silently in CI for thirteen
    // versions, and it is the same failure — a check that has never run looks exactly like one that
    // passes.
    let reserved = typeck
        .split_once("fn is_reserved_name")
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(body, _)| body)
        .expect("`fn is_reserved_name` in src/typeck.rs — the built-in name list");
    let builtins: Vec<String> = reserved
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .map(|w| w.to_string())
        .collect();
    assert!(
        builtins.len() > 10,
        "failed to read the built-in names out of src/typeck.rs (found {:?}). They moved — find \
         them and fix this scrape rather than deleting it: an empty list makes this test pass by \
         checking nothing, which is how `exit` stayed missing from the grammar.",
        builtins
    );
    words.extend(builtins);

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
    //
    // The floor moved DOWN a SECOND time in v0.0.146, from 191 to 189, and this one is not a
    // shortfall — it is nine fixtures becoming invalid because the rule they tested no longer
    // exists. M14 slice 2 removed the requirement that allocation happen inside a region, so
    // `slice_needs_region`, `string_concat_needs_region`, `substring_needs_a_region`,
    // `interp_value_needs_region`, `read_file_needs_region`, `slice_taints_struct`,
    // `allocates_call_needs_a_region`, `allocates_method_needs_a_region` and
    // `allocates_through_a_trait_object_needs_a_region` all describe programs that are now
    // correct. They were retired, and `tests/pass/no_region_needed.bx` demonstrates every one
    // of the nine cases instead — so the coverage moved rather than vanishing.
    //
    // Two of the nine were among the 191 stage-1 caught, hence 191 - 2. The denominator fell
    // by nine and the numerator by two, which is the arithmetic of retiring fixtures the
    // second compiler had never learned to reject in the first place.
    //
    // The rule that REMAINS is the one worth keeping the fixtures for: a value built inside a
    // `region` block still cannot leave it, because that block still releases.
    // `allocates_cannot_escape_inner_region` and `allocates_cannot_escape_via_a_binding` cover
    // the two spellings of it.
    //
    // v0.0.165 raised it to 199, and how far it had DRIFTED is the more interesting number. The
    // floor said 189; stage-1 was actually rejecting 195. Six fixtures' worth of progress had
    // accumulated above the line, where — exactly as the note above predicts — a regression in any
    // of them would have gone unnoticed. Privacy enforcement in stage-1 added the three
    // `private_*` fixtures and a fourth written the same day, and the floor was moved to the
    // measured value rather than to the measured value minus a cushion. A cushion is the drift.
    //
    // The fourth is worth naming: `private_literal_bypasses_the_constructor` did not exist. The
    // rule shipped in v0.0.151 and stage-0 enforced it correctly for fourteen versions with
    // nothing in the suite saying so, and it surfaced only because a second implementation had to
    // be told the same rule. That is the argument for stage-1 in one sentence.
    //
    // v0.0.167 raised it to 204, and noted that all five of the new `bracket_*` fixtures were caught
    // for the WRONG reason: stage-1 could not parse contract brackets at all, so the parse failure
    // counted. The note said they would need re-earning.
    //
    // v0.0.169 earned them. Stage-1 parses brackets now and reproduces every one of the five
    // refusals in its own words — the `it` collision, `it` outside a bracket, a clause that is not
    // pure, a clause that is not a Bool, and `[]` promising nothing. The number did not move, which
    // is the point of having written the note: without it, a floor that held would have looked like
    // nothing happening rather than five fixtures changing hands.
    //
    // v0.0.183 raised it to 210, and that one is five fixtures earned in a single change: stage-1
    // enforces `touches` now, so `effect_not_declared`, `effect_not_declared_transitively`,
    // `pure_cannot_touch`, `unknown_effect` and the new `effect_not_declared_through_a_method` are all
    // caught by its checker rather than slipping past a rule it did not have.
    //
    // The last of those five is a bug stage-1 found IN STAGE-0, which is the direction that matters:
    // `method_effects` was enforced inward and never outward, so an effect could vanish from a
    // signature chain by being called through a method.
    //
    // v0.0.170 did the same for `match` on a scalar: four `match_scalar_*` fixtures that stage-1 had
    // been rejecting as PARSE errors (it refused a literal pattern outright) are now rejected by its
    // checker, each naming the rule it broke. Again the count is unchanged, and again that is only
    // legible because the previous version wrote down which ones were borrowed.
    //
    // The lesson about the instrument, now twice over: a floor cannot tell you that a fixture changed
    // hands. Only a note in the version that borrowed it can, so write the note.
    //
    // v0.0.179 raised it to 205, and the arithmetic is a fixture RETIRED and two added.
    // `enum_cannot_carry_an_enum` described a program that is now CORRECT — the rule it tested was a
    // proxy for finite width, and `enum Outer { Held(Inner) }` is finite. Its coverage moved into
    // `tests/pass/enum_payload_finite_width.bx`, the same way M14 moved nine region fixtures into
    // `no_region_needed.bx`. `enum_carries_itself_by_value` and `enums_carry_each_other_by_value`
    // replaced it, and both are caught for the right reason in both compilers.
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
        caught >= 224,
        "stage-1 rejected only {} of {} fail programs, down from 224",
        caught,
        total
    );
    assert_eq!(
        element_errors, 1,
        "stage-1 must reject a [String] where a [Int] is wanted — an element type is part \
         of the type, and comparing two slices as equal made them interchangeable"
    );
    assert_eq!(
        shape_errors, 3,
        "stage-1 should have caught the arity, the element type and the indexed String. \
         There were FOUR until v0.0.146: the fourth was `to_string(...)` with no region open, \
         and M14 slice 2 deleted that rule rather than stage-1 forgetting it — nothing needs a \
         region in order to allocate. Lowered deliberately, like the 191 -> 189 ratchet above"
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
            "class Point { x: Int, y: Int }\n             class Line { from: Point, to: Point, label: String }\n             function total_of(p: Point) -> Int { return p.x + p.y; }\n             let a: Point = Point { x: 3, y: 4 };\n             let mutable b: Point = a;\n             b.x = 100;\n             print(total_of(a));\nprint(a.x);\nprint(b.x);\n             let l: Line = Line { from: a, to: b, label: \"diagonal\" };\n             print(l.from.x);\nprint(l.to.x);\nprint(l.label);\n             let mutable xs: [Int; 4] = [10, 20, 30, 40];\n             xs[1] = 99;\n             let mutable i: Int = 0;\nlet mutable total: Int = 0;\n             while i < 4 { total = total + xs[i]; i = i + 1; }\n             print(total);\n",
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
        ("struct ", "class"),
        ("trait ", "interface"),
        ("record ", "class"),
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
        "class Tok { kind: Int, start: Int }\nfunction scan(text: String) -> Int { return len(text); }\n",
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
             region r {{\n  print(string_find(\"hello, modules\", \"modules\"));\n               print(string_trim(\"   padded   \"));\n  print(string_to_int(\"-42\", 0));\n               print(string_to_int(\"nope\", 99));\n               print(string_join(string_split(\"a, b, c\", \", \"), \" | \"));\n               let wrote: Int = file_write(\"{1}/demo.txt\", \"first\\n\");\n               let more: Int = file_append(\"{1}/demo.txt\", \"second\\n\");\n               print(len(file_read(\"{1}/demo.txt\")));\n               print(file_exists(\"{1}/demo.txt\"));\n               print(len(file_list_directory(\"{0}\")) >= 3);\n               print(os_run(\"true\"));\n  print(string_trim(os_capture(\"echo captured\")));\n}}\n",
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
        "7\npadded\n-42\n99\na | b | c\n13\ntrue\ntrue\n0\ncaptured\n",
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

    // The tarball for THIS version, not whichever one `read_dir` happens to return first.
    //
    // It used to be `find(|p| ends_with(".tar.gz"))`, and that passed only because `dist/` held
    // exactly one file. With three old artifacts lying beside it the test unpacked a v0.0.83 binary
    // and certified that — a release test that green-lights the wrong compiler, which is worse than
    // one that fails, because the whole point of it is to be the last thing between a build and
    // somebody's machine.
    let version = env!("CARGO_PKG_VERSION");
    let wanted = format!("burxt-{}-", version);
    let tarball = fs::read_dir(root.join("dist"))
        .expect("dist/")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            name.starts_with(&wanted) && name.ends_with(".tar.gz")
        })
        .unwrap_or_else(|| panic!("no dist/{}*.tar.gz — release.sh should have just written it", wanted));

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
        // 480 MB from v0.0.185, and the TREND is what this number is for rather than the value.
        // 196 MB at v0.0.90, 239 at v0.0.110, 335 at v0.0.121, 392 at v0.0.168, 400 at v0.0.169,
        // 440 at v0.0.169 — which is roughly 40 KB of peak RSS per line of compiler, growing with
        // the source and nothing else. M13's brackets added ~290 lines and 8 MB, exactly on that
        // line. v0.0.183's `touches` enforcement in stage-1 added ~400 lines and ~16 MB, also on it.
        //
        // The ceiling is raised rather than the growth fixed, and it is worth being clear about why
        // that is a decision and not an accident. Nothing here LEAKS: stage-1 allocates every
        // String, node and token into one bump region and never releases, because there is nothing
        // to release into — per-block release is M14 slice 3, still open, and it is the fix. Until
        // then peak RSS is the total ever allocated, which is a linear function of the input.
        //
        // The 440 raise was too small, and HOW it failed is the part worth keeping. v0.0.183 landed
        // at 440 MB on the CI runner and 436 MB on a developer laptop — so the same commit failed
        // in one place and passed in the other, and the first read of that is "the docs branch broke
        // the compiler". A ceiling with 1% of margin is not measuring the trend it exists to
        // measure; it is measuring the machine. 480 leaves ~1,100 lines of headroom, which is a
        // milestone's worth rather than a rounding error's.
        //
        // The arithmetic that says when this stops being a raise: the region reserves 1 GB, so at
        // 40 KB per line the wall is around 25,000 lines of Burxt. Stage-1 is 10,400. Two more
        // milestones of this size and the answer has to be slice 3 rather than a bigger number.
        //
        // **v0.0.199 is the first of those two.** 497 MB measured, deterministically — three runs,
        // the same number each time — from the pointer wall (v0.0.196) and the bit operations and hex
        // lexing (v0.0.199), which added roughly 400 lines across both compilers. Raised to 540
        // rather than to 500, and the paragraph above is the reason: 3 MB of margin on a 497 MB
        // measurement is 0.6%, which is precisely the mistake the 440 raise made. A ceiling that
        // close measures the runner, not the trend, and the failure mode is a commit that passes
        // here and fails in CI while looking like someone else's fault.
        //
        // So: ONE more raise of this size, and then the answer is slice 3. Writing that here rather
        // than in a commit message, because the next person to reach this line is the one who needs
        // to know the budget is nearly spent.
        assert!(
            kb < 540 * 1024,
            "the compiler's peak RSS on its own source is {} MB; the ceiling is 540 MB, and \
             the region it reserves is 1 GB (196 MB at v0.0.90, 239 MB at v0.0.110, 335 MB at \
             v0.0.121, 400 MB at v0.0.169, 440 MB at v0.0.183, 480 MB at v0.0.190, 497 MB at \
             v0.0.199 — roughly 40 KB per line of compiler, and nothing releases until the process \
             exits because per-block release is M14 slice 3)",
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
// element, a bound, an interface bound, a generic calling a generic), generics_types (two type
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
    // library, and each module of the compiler. Two directories are excluded on purpose, and for
    // the same reason: `examples/negative/` and `examples/refused/` are meant to be wrong. A
    // squiggle there is the point — `refused/` exists to show a reviewer ten mistakes an agent
    // writes confidently, so a clean editor on those files would mean the page had stopped being
    // true. `the_refusals_page_is_not_stale` asserts the opposite of this test about them.
    let mut sources: Vec<PathBuf> = Vec::new();
    for dir in ["examples", "lib"] {
        collect_bx(&root.join(dir), &mut sources);
    }
    sources.retain(|p| {
        !p.components().any(|c| c.as_os_str() == "negative" || c.as_os_str() == "refused")
    });
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
         class Stack<T> { items: [T] }\n\
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
    for target in ["guide", "reference", "examples", "install"] {
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

    // The generated examples page is current. The generator is handed the binary CARGO built for
    // this test rather than looking for a release one itself, which is what it used to do — and in
    // CI, which builds debug, that meant this check SKIPPED for thirteen versions. A check that has
    // never run looks exactly like one that passes, so it no longer has a way to opt out.
    let checked = Command::new("python3")
        .arg("scripts/site-examples.py")
        .arg("--check")
        .env("BURXT", env!("CARGO_BIN_EXE_burxt"))
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

/// Every guide page teaches the same way, in the same order.
///
/// The guide used to be twelve pages of ad-hoc headings. Each was individually fine and collectively
/// unnavigable: no page told you where its limitations were, seven of them never said what the feature
/// costs, and none of them had a worked example you could run. A reader who wanted "what does this cost
/// me" had to read the prose and hope.
///
/// So every page now walks one ladder — what the problem is, an analogy, a step closer, the mechanics,
/// the design reason, the costs, the use cases, examples — and the ladder is enforced rather than
/// remembered, because the eighth page written six months from now is the one that would quietly skip
/// three steps.
///
/// The `Examples` step is the one worth being strictest about. It is the step that turns a page from an
/// explanation into something a reader can check, and it is the easiest to leave out.
#[test]
fn every_guide_page_teaches_in_eight_steps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // In order. The analogy step is matched by prefix, because it keeps its own wording — "Think of a
    // tray", "Think of a cloakroom" — and that voice is worth more than a uniform heading.
    const LADDER: [&str; 8] = [
        "What this is for",
        "Think of ",
        "A step closer",
        "In code",
        "Why it is built this way",
        "What it costs",
        "When you reach for it",
        "Examples",
    ];

    let mut pages: Vec<PathBuf> = fs::read_dir(root.join("docs/guide"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.len() > 3 && n.starts_with(|c: char| c.is_ascii_digit()) && n.ends_with(".md"))
        })
        .collect();
    pages.sort();
    assert!(pages.len() >= 12, "expected at least twelve numbered guide pages, found {}", pages.len());

    let mut problems = Vec::new();
    for page in &pages {
        let name = page.file_name().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(page).unwrap();

        // Headings only, and never one inside a fence — `## ` at the start of a line is ordinary
        // shell output in a code block.
        let mut headings: Vec<String> = Vec::new();
        let mut fenced = false;
        for line in text.lines() {
            if line.starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if !fenced && line.starts_with("## ") {
                headings.push(line[3..].trim().to_string());
            }
        }

        let mut at = 0usize;
        for step in LADDER {
            let found = headings[at..].iter().position(|h| {
                if step.ends_with(' ') { h.starts_with(step) } else { h == step }
            });
            match found {
                Some(offset) => at += offset + 1,
                None => problems.push(format!(
                    "docs/guide/{} has no `## {}` after the steps before it. The ladder is: {}",
                    name,
                    step.trim_end(),
                    LADDER.join(" → ")
                )),
            }
        }

        // `Next` closes every page, and `the_guide_reads_in_order` already checks it links forward.
        if !headings.iter().any(|h| h == "Next") {
            problems.push(format!("docs/guide/{} does not end with a `## Next`", name));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every guide page draws its analogy rather than only describing it.
///
/// The report that started this was "I don't see any analogy here" — on pages that *had* one, in prose.
/// Eight of the twelve already said "think of a tray" or "think of a cloakroom" and then showed a
/// schematic of a bump pointer, which is the mechanism rather than the metaphor. So the analogy step now
/// carries a picture of the everyday object, and this is what keeps it there.
///
/// Two things are checked beyond its existence, and both were mistakes made while drawing these. A
/// `viewBox` with no `max-width` overflows a phone. And an SVG is invisible to a screen reader without a
/// real label — `role="img"` with an `aria-label` that says what the picture shows, not "diagram".
#[test]
fn every_guide_page_shows_an_analogy_picture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut problems = Vec::new();
    let mut drawn = 0;

    for entry in fs::read_dir(root.join("docs/guide")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !(name.ends_with(".md") && name.starts_with(|c: char| c.is_ascii_digit())) {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();

        // The analogy step runs from its own heading to the next `## `.
        let Some(start) = text.find("\n## Think of ") else {
            problems.push(format!("docs/guide/{} has no `## Think of …` step", name));
            continue;
        };
        let rest = &text[start + 1..];
        let section = match rest[3..].find("\n## ") {
            Some(end) => &rest[..end + 3],
            None => rest,
        };

        let Some(svg) = section.find("<svg") else {
            problems.push(format!(
                "docs/guide/{}'s analogy step has no picture. The prose says what to think of; the \
                 report was that there was nothing to look at.",
                name
            ));
            continue;
        };
        let svg = &section[svg..];
        drawn += 1;

        if !svg.contains("viewBox") || !svg.contains("max-width") {
            problems.push(format!(
                "docs/guide/{}'s analogy picture needs both a `viewBox` and `max-width:100%` — \
                 without them it does not scale on a phone",
                name
            ));
        }
        if !svg.contains("role=\"img\"") {
            problems.push(format!("docs/guide/{}'s analogy picture has no `role=\"img\"`", name));
        }
        // A label, and a real one. "diagram" and "illustration" describe the medium, not the picture.
        match svg.split_once("aria-label=\"") {
            Some((_, tail)) => {
                let label = tail.split('"').next().unwrap_or("");
                if label.len() < 40 {
                    problems.push(format!(
                        "docs/guide/{}'s analogy picture is labelled {:?} — a screen reader gets only \
                         this, so it has to say what the picture SHOWS",
                        name, label
                    ));
                }
            }
            None => problems.push(format!(
                "docs/guide/{}'s analogy picture has no `aria-label`",
                name
            )),
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(drawn >= 12, "only {} guide pages draw their analogy", drawn);
}

/// The PHP, Python and Rust ports of the till print exactly what the Burxt one prints.
///
/// The examples page shows the same point-of-sale program four times and says the ports agree. That
/// is the whole comparison — if they printed different totals, the page would be comparing four
/// programs rather than one program written four ways, and the argument about where the rounding rule
/// lives would be worthless.
///
/// The claim lives here rather than on the page on purpose. Running `php`, `python3` and `rustc`
/// while GENERATING would make the page's bytes depend on which runtimes were installed, so CI and a
/// laptop would produce different files and the staleness check would fail for a reason that has
/// nothing to do with the site.
///
/// A missing runtime SKIPS, and says which — the alternative is a test that only ever runs on one
/// machine, and this file already records what that costs.
#[test]
fn the_ports_agree_with_the_original() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("pos-ports");
    fs::create_dir_all(&scratch).unwrap();

    // The Burxt program is the reference, so it is not optional.
    let built = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("run")
        .arg("till.bx")
        .arg("-o")
        .arg(scratch.join("till"))
        .current_dir(root.join("examples/pos"))
        .output()
        .expect("burxt run");
    assert!(
        built.status.success(),
        "examples/pos/till.bx no longer runs:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let expected: String = String::from_utf8_lossy(&built.stdout)
        .lines()
        .filter(|l| !l.starts_with("compiled "))
        .map(|l| format!("{}\n", l.trim_end()))
        .collect();
    assert!(
        expected.contains("230.46"),
        "the till's own output changed — check this test's reference before the ports:\n{}",
        expected
    );

    let mut skipped = Vec::new();
    let mut wrong = Vec::new();

    let mut compare = |what: &str, got: std::process::Output| {
        let shown: String = String::from_utf8_lossy(&got.stdout)
            .lines()
            .map(|l| format!("{}\n", l.trim_end()))
            .collect();
        if !got.status.success() {
            wrong.push(format!(
                "the {} port did not run:\n{}",
                what,
                String::from_utf8_lossy(&got.stderr)
            ));
        } else if shown != expected {
            wrong.push(format!(
                "the {} port prints something different from the Burxt one.\nBurxt:\n{}\n{}:\n{}",
                what, expected, what, shown
            ));
        }
    };

    // PHP and Python are interpreters: point them at the entry file in its own directory, because
    // each port requires or imports its siblings by relative name.
    match Command::new("php").arg("till.php").current_dir(root.join("examples/pos-php")).output() {
        Ok(out) => compare("PHP", out),
        Err(_) => skipped.push("php"),
    }
    match Command::new("python3")
        .arg("till.py")
        .current_dir(root.join("examples/pos-python"))
        .output()
    {
        Ok(out) => compare("Python", out),
        Err(_) => skipped.push("python3"),
    }

    // Rust has to be compiled. `till.rs` declares its siblings as modules, so one rustc invocation
    // over the entry file is the whole build.
    match Command::new("rustc")
        .arg("--edition=2021")
        .arg("-O")
        .arg("till.rs")
        .arg("-o")
        .arg(scratch.join("till-rust"))
        .current_dir(root.join("examples/pos-rust"))
        .output()
    {
        Ok(out) if out.status.success() => {
            match Command::new(scratch.join("till-rust")).output() {
                Ok(ran) => compare("Rust", ran),
                Err(e) => wrong.push(format!("the Rust port built and would not run: {}", e)),
            }
        }
        Ok(out) => wrong.push(format!(
            "the Rust port does not compile:\n{}",
            String::from_utf8_lossy(&out.stderr)
        )),
        Err(_) => skipped.push("rustc"),
    }

    let _ = fs::remove_dir_all(&scratch);
    if !skipped.is_empty() {
        eprintln!("skipped the {} port(s): not installed", skipped.join(", "));
    }
    assert!(
        wrong.is_empty(),
        "the examples page says these print the same thing:\n\n{}",
        wrong.join("\n\n")
    );
}

/// The site does not quote a tool saying something the tool does not say.
///
/// The landing page and guide page 12 both show output from `burxt review` and `burxt mcp-schema`,
/// and those blocks are the argument — not decoration around it. The landing page's `burxt review`
/// block was INVENTED: close enough to pass a read, wrong in three ways. It named a method
/// `Account.withdraw` that was called `withdrawn`, it used column widths the tool does not use, and
/// it omitted the summary line the tool always prints. Nobody would have caught that by proofreading,
/// because it looked exactly like real output.
///
/// The guide has already lied twice this way — two error messages the compiler never produced — and
/// both times running the example is what caught it. So: run the tools, and check the page against
/// what came back.
#[test]
fn the_site_quotes_the_tools_honestly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("site-tools");
    fs::create_dir_all(&scratch).unwrap();

    // ---- `burxt review` -----------------------------------------------------------------------
    //
    // The page shows four verdict lines. This builds the before/after pair that produces exactly
    // those four, so a changed output FORMAT — a column width, a wording, the summary — fails here
    // rather than sitting on the front page looking plausible.
    let before = "class Account {\n    owner: String,\n    private balance: Decimal<2>,\n\n\
         \x20   function (self) withdrawn(amount: Decimal<2>) -> Decimal<2>\n\
         \x20       requires amount > $0.00\n\
         \x20       requires amount <= self.balance\n\
         \x20   { return self.balance - amount; }\n}\n\n\
         function invoice_total(net: Decimal<2>) -> Decimal<2> {\n    return net;\n}\n\n\
         function line_tax(quantity: Int, unit: Decimal<2>) -> Decimal<2> {\n\
         \x20   return unit * quantity;\n}\n";
    let after = "class Account {\n    owner: String,\n    balance: Decimal<2>,\n\n\
         \x20   function (self) withdrawn(amount: Decimal<2>) -> Decimal<2>\n\
         \x20       requires amount > $0.00\n\
         \x20   { return self.balance - amount; }\n}\n\n\
         function invoice_total(net: Decimal<2>) -> Decimal<2> touches network {\n    return net;\n}\n\n\
         function line_tax(quantity: Int [> 0], unit: Decimal<2>) -> Decimal<2> {\n\
         \x20   return unit * quantity;\n}\n";
    fs::write(scratch.join("before.bx"), before).unwrap();
    fs::write(scratch.join("after.bx"), after).unwrap();

    let reviewed = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("review")
        .arg("before.bx")
        .arg("after.bx")
        .current_dir(&scratch)
        .output()
        .expect("burxt review");
    let said = String::from_utf8_lossy(&reviewed.stdout).to_string();
    assert_eq!(
        reviewed.status.code(),
        Some(1),
        "`burxt review` must exit 1 when a promise got weaker — the landing page calls it a gate \
         rather than a report, and a gate that exits 0 is a report"
    );

    let landing = fs::read_to_string(root.join("docs/index.md")).expect("the landing page");
    let mut wrong = Vec::new();
    for line in said.lines().filter(|l| !l.trim().is_empty()) {
        if !landing.contains(line) {
            wrong.push(format!("`burxt review` printed this and docs/index.md does not:\n    {}", line));
        }
    }

    // ---- `burxt mcp-schema` -------------------------------------------------------------------
    //
    // The pages pretty-print the manifest across several lines for reading, which is presentation
    // rather than invention — so what is held here is every FACT in it: each key and value must
    // appear in what the compiler actually emitted.
    let schema = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("mcp-schema")
        .arg(root.join("examples/mcp/tools.bx"))
        .current_dir(root)
        .output()
        .expect("burxt mcp-schema");
    let manifest = String::from_utf8_lossy(&schema.stdout).to_string();
    assert!(
        manifest.contains("\"name\":\"line_total\""),
        "`burxt mcp-schema` no longer describes `line_total`:\n{}",
        manifest
    );

    let page12 = fs::read_to_string(root.join("docs/guide/12-tools-and-agents.md"))
        .expect("guide page 12");
    for claim in [
        "\"exclusiveMinimum\":\"0.00\"",
        "\"maximum\":\"100000\"",
        "\"type\":\"integer\"",
        "\"description\":\"Decimal<2>\"",
    ] {
        if !manifest.contains(claim) {
            wrong.push(format!(
                "the site shows `{}` in the derived schema and the compiler does not emit it",
                claim
            ));
        }
        if !landing.contains(claim) && !page12.contains(claim) {
            wrong.push(format!("neither page shows `{}`, which the schema turns on", claim));
        }
    }

    // The skipped-clause note, which is the honest half of page 12 and the easiest thing to quietly
    // stop printing.
    fs::write(
        scratch.join("relational.bx"),
        "function withdraw(balance: Decimal<2>, amount: Decimal<2> [<= balance]) -> Decimal<2>\n\
         { return balance - amount; }\n",
    )
    .unwrap();
    let relational = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("mcp-schema")
        .arg("relational.bx")
        .current_dir(&scratch)
        .output()
        .expect("burxt mcp-schema");
    let note = String::from_utf8_lossy(&relational.stderr).to_string();
    let note = note.trim();
    if !note.is_empty() && !page12.contains(note) {
        wrong.push(format!(
            "`burxt mcp-schema` reports this on stderr and page 12 quotes something else:\n    \
             {}\nThat note IS the page's claim about what the tool cannot express.",
            note
        ));
    }

    let _ = fs::remove_dir_all(&scratch);
    assert!(
        wrong.is_empty(),
        "the site quotes these tools inaccurately:\n\n{}",
        wrong.join("\n\n")
    );
}

/// The reference is what the compiler says it is, and the sidebar is what the pages say it is.
///
/// The page these replaced was hand-written, and its own header claimed it had been "generated by
/// reading the compiler, not by memory". That was true of the afternoon somebody wrote it. By the
/// time anyone looked it listed `record` as a keyword — renamed to `class` eleven versions earlier —
/// and called `for` and `is` "reserved but not yet used" ninety versions after `for x in xs`
/// shipped. A reference is the one page a reader trusts literally, so it is the worst page to let
/// rot, and prose does not fail to compile.
///
/// So both are generated and both are diffed, exactly like `the_refusals_page_is_not_stale` and the
/// examples page. The generator does more than substitute text: it compiles a use of every builtin
/// signature it prints, and it holds its own list against `is_reserved_name`, so adding a builtin to
/// the language fails HERE until the reference knows about it.
#[test]
fn the_reference_is_not_stale() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for (script, what) in [
        ("scripts/site-reference.py", "docs/reference/ and docs/assets/search.json"),
        ("scripts/site-nav.py", "docs/_data/nav.yml"),
    ] {
        let checked = Command::new("python3")
            .arg(script)
            .arg("--check")
            .env("BURXT", env!("CARGO_BIN_EXE_burxt"))
            .current_dir(root)
            .output()
            .unwrap_or_else(|e| panic!("running {}: {}", script, e));
        assert!(
            checked.status.success(),
            "{} no longer matches the compiler. Regenerate it:\n    python3 {}\n{}{}",
            what,
            script,
            String::from_utf8_lossy(&checked.stdout),
            String::from_utf8_lossy(&checked.stderr)
        );
    }

    let nav = fs::read_to_string(root.join("docs/_data/nav.yml")).expect("the sidebar data");
    assert!(
        nav.contains("- title: The guide") && nav.contains("- title: Reference"),
        "docs/_data/nav.yml lost one of its two groups"
    );
    let steps = nav.matches("          id: ").count();
    assert!(steps > 80, "the sidebar lists only {} steps, which cannot be right", steps);

    // Every anchor the SEARCH box points at is one the SIDEBAR also lists.
    //
    // This is the invariant worth holding, because the failure it prevents is invisible: an anchor
    // that does not exist still loads the page, just at the top of it, so a wrong link and a right
    // link look identical unless you know which section you expected. Two generators computing the
    // same id two ways is how that happens — and it nearly did. kramdown DELETES underscores rather
    // than hyphenating them, so `to_string` is `#tostring`, and the first version of the reference
    // pointed every link at `#to-string`, which exists nowhere.
    //
    // Two things now make it safe, and this checks both: the generated pages state their ids
    // outright with `{: #…}` rather than predicting them, and both generators get every id from one
    // function.
    let search = fs::read_to_string(root.join("docs/assets/search.json")).expect("the search index");
    let listed: std::collections::HashSet<&str> = nav
        .lines()
        .filter_map(|l| l.trim().strip_prefix("id: "))
        .collect();
    let mut orphans = Vec::new();
    for row in search.split("\"url\": \"").skip(1) {
        let Some((url, _)) = row.split_once('"') else { continue };
        // Only the anchors into a guide page: a reference anchor is checked by the `{: #…}` sweep
        // below, and `/reference/#keywords` points at a heading this data file does not enumerate.
        if !url.starts_with("/guide/") {
            continue;
        }
        let Some((_, fragment)) = url.split_once('#') else { continue };
        if !listed.contains(fragment) {
            orphans.push(url.to_string());
        }
    }
    orphans.sort();
    orphans.dedup();
    assert!(
        orphans.is_empty(),
        "the search index points at {} anchor(s) the sidebar does not list, so one of the two is \
         wrong and both will look like they work: {:?}\nBoth ids must come from `headings` in \
         scripts/site-nav.py.",
        orphans.len(),
        orphans
    );

    // And the generated pages really do state their ids, rather than leaving them to be guessed.
    let builtins = fs::read_to_string(root.join("docs/reference/builtins.md")).expect("builtins");
    let stated = builtins.matches("\n{: #").count();
    let headings = builtins.matches("\n## ").count();
    assert_eq!(
        stated, headings,
        "docs/reference/builtins.md has {} headings but states {} ids. Every generated heading must \
         carry `{{: #…}}`, because kramdown's own slug rule is not what anyone would guess — it \
         turns `to_string` into `tostring`.",
        headings, stated
    );
}

/// The website's highlighter and the compiler must not drift either.
///
/// `docs/assets/burxt-editor.js` colours all 92 Burxt blocks on the site, and it is a second
/// implementation of the same word lists the lexer holds — which is exactly the arrangement that
/// rots. A keyword the compiler knows and the site does not is a word that compiles and renders as
/// a plain identifier, which is how documentation starts looking unfinished.
///
/// The editor grammar already has this test (`editor_grammar_knows_every_keyword_the_compiler_does`)
/// and this is deliberately its twin, including the part that matters most: it reads the compiler's
/// own tables out of the source rather than restating them here, because a restated list is the
/// thing that drifts.
///
/// It searches only the WORD LISTS, never the prose. The grammar's version learned that by
/// mutation — the looser "anywhere in the file" form passed after a rule was deleted, because the
/// word survived in a comment — and this file is even more comment than that one.
#[test]
fn the_web_highlighter_knows_every_keyword_the_compiler_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lexer = fs::read_to_string(root.join("src/lexer.rs")).unwrap();
    let typeck = fs::read_to_string(root.join("src/typeck.rs")).unwrap();
    let js = fs::read_to_string(root.join("docs/assets/burxt-editor.js"))
        .expect("docs/assets/burxt-editor.js — the site's highlighter");

    // Keywords, from the lexer's `"word" => Token::Variant` table.
    let mut want: Vec<String> = lexer
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix('"')?;
            let (word, tail) = rest.split_once('"')?;
            tail.trim_start().starts_with("=> Token::").then(|| word.to_string())
        })
        .collect();
    assert!(
        want.len() > 20,
        "failed to read the keyword table out of src/lexer.rs (found {:?})",
        want
    );

    // Built-in names, from `is_reserved_name` in the typechecker. Same scrape as the grammar test,
    // and same reason for the floor: an empty list would make this pass by checking nothing.
    let reserved = typeck
        .split_once("fn is_reserved_name")
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(body, _)| body)
        .expect("`fn is_reserved_name` in src/typeck.rs");
    let builtins: Vec<String> = reserved
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .map(|w| w.to_string())
        .collect();
    assert!(builtins.len() > 10, "failed to read the built-in names (found {:?})", builtins);
    want.extend(builtins);

    // The spellings the language RENAMED, from `renamed_keyword`. These do not compile, so the site
    // must colour them as the errors they are rather than as identifiers — and they are not in the
    // keyword table, which is precisely why the grammar had gone years without `trait` and `record`.
    let renamed = lexer
        .split_once("fn renamed_keyword")
        .and_then(|(_, rest)| rest.split_once("_ => return None"))
        .map(|(body, _)| body)
        .expect("`fn renamed_keyword` in src/lexer.rs");
    let old: Vec<String> = renamed
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix('"')?;
            let (word, tail) = rest.split_once('"')?;
            tail.trim_start().starts_with("=>").then(|| word.to_string())
        })
        .collect();
    assert!(
        old.len() >= 6,
        "failed to read the renamed spellings out of src/lexer.rs (found {:?})",
        old
    );
    want.extend(old);

    // Only what is inside a `words('...')` call. A word in a comment is not a word that highlights.
    let lists: String = js
        .split("words(")
        .skip(1)
        .filter_map(|chunk| chunk.split_once(')').map(|(args, _)| args.to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        lists.len() > 500,
        "failed to read the word lists out of docs/assets/burxt-editor.js (got {} bytes). They are \
         the strings passed to `words(...)`; if that shape changed, fix this scrape rather than \
         loosening it to search the whole file — most of that file is comment.",
        lists.len()
    );

    let known = |w: &str| {
        // A word, not a substring: `as` must not be satisfied by `class`, and `push` must not be
        // satisfied by nothing at all.
        lists.match_indices(w).any(|(i, _)| {
            let before = lists[..i].chars().next_back();
            let after = lists[i + w.len()..].chars().next();
            let boundary = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            boundary(before) && boundary(after)
        })
    };

    let missing: Vec<&String> = want.iter().filter(|w| !known(w)).collect();
    assert!(
        missing.is_empty(),
        "these words are known to the compiler but absent from the website's highlighter: {:?}\n\
         Add them to a word list in docs/assets/burxt-editor.js — a keyword the compiler knows and \
         the site does not renders as a plain identifier on all {} Burxt blocks.",
        missing,
        92
    );
}

/// No text on the site is too faint to read, and the grey that was cannot come back.
///
/// The site shipped with `--ink-soft: #6e6e73` carrying the navigation, the hero subtitle, every
/// table header, every caption and the whole footer. At 5.1:1 that passes a checker and still reads
/// as washed out at the 13px most of it was used at, so the page looked faint in exactly the places
/// where it was explaining itself. The report was "there are no colours and the grey is not visible".
///
/// Contrast is arithmetic, so it does not need an eye — but nothing was doing the arithmetic, and a
/// pale colour is the kind of regression that reaches the live site because every test stays green
/// and the page merely looks a bit tired. So: read the palette out of the stylesheet, and hold each
/// text colour to WCAG AA against the surface it is actually used on.
///
/// The syntax palette is included deliberately. A theme is text — a pretty pale keyword is still
/// unreadable text, and a code block is the thing on this site people came to read.
#[test]
fn the_site_text_is_readable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let css = fs::read_to_string(root.join("docs/assets/site.css")).expect("the stylesheet");

    // WCAG 2.1's relative luminance and contrast ratio, straight from the specification.
    fn channel(eight_bit: u8) -> f64 {
        let c = eight_bit as f64 / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    fn luminance(hex: &str) -> f64 {
        let n = u32::from_str_radix(hex, 16).unwrap();
        let (r, g, b) = ((n >> 16) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8);
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }
    fn ratio(a: &str, b: &str) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    // `--name: #rrggbb;` from the `:root` block, and `.t-x { color: #rrggbb; }` for the syntax
    // classes. Read out of the file rather than listed here, because a list here is what drifts.
    let six = |value: &str| -> Option<String> {
        let hex = value.trim().trim_start_matches('#');
        let hex = hex.split(|c: char| !c.is_ascii_hexdigit()).next().unwrap_or("");
        (hex.len() == 6).then(|| hex.to_ascii_lowercase())
    };
    let mut vars: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut tokens: Vec<(String, String)> = Vec::new();
    for line in css.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("--") {
            if let Some((name, value)) = rest.split_once(':') {
                if let Some(hex) = six(value) {
                    vars.insert(format!("--{}", name.trim()), hex);
                }
            }
        }
        // `.t-kw       { color: #9b2393; }` — one class, one declaration, one line.
        if line.starts_with(".t-") {
            if let Some((selector, body)) = line.split_once('{') {
                if let Some((_, value)) = body.split_once("color:") {
                    if let Some(hex) = six(value) {
                        tokens.push((selector.trim().to_string(), hex));
                    }
                }
            }
        }
    }
    let of = |name: &str| -> String {
        vars.get(name).cloned().unwrap_or_else(|| panic!("docs/assets/site.css lost {}", name))
    };
    let paper = of("--paper");
    let wash = of("--wash");

    let mut faint = Vec::new();
    let mut check = |what: &str, ink: &str, on: &str, floor: f64| {
        let got = ratio(ink, on);
        if got < floor {
            faint.push(format!(
                "{} is #{} on #{} — {:.2}:1, and text needs {:.1}:1",
                what, ink, on, got, floor
            ));
        }
    };

    // Prose, links, and the one secondary tier that chrome is allowed to use.
    check("--ink", &of("--ink"), &paper, 4.5);
    check("--ink-2", &of("--ink-2"), &paper, 4.5);
    check("--accent", &of("--accent"), &paper, 4.5);
    check("--refuse", &of("--refuse"), &paper, 4.5);
    // White on the accent pill, which is the site's one filled control.
    check("white on --accent", "ffffff", &of("--accent"), 4.5);
    // Anything sitting on a panel rather than on the page.
    check("--ink-2 on --wash", &of("--ink-2"), &wash, 4.5);
    check("--refuse on --wash", &of("--refuse"), &wash, 4.5);

    assert!(!tokens.is_empty(), "failed to read the syntax palette out of docs/assets/site.css");
    for (class, hex) in &tokens {
        check(class, hex, &wash, 4.5);
    }

    assert!(faint.is_empty(), "text on the site is too faint to read:\n  {}", faint.join("\n  "));

    // And the grey itself is gone. The variable is DELETED rather than darkened, so nothing can
    // quietly keep reaching for it — a darkened `--ink-soft` would have been re-used for prose
    // within a version. Its name is allowed to appear in the comment that records why, which is
    // why this looks for a declaration and a use rather than for the string anywhere.
    let mut ghosts = Vec::new();
    for entry in walk(&root.join("docs")) {
        let text = match fs::read_to_string(&entry) {
            Ok(t) => t,
            Err(_) => continue,
        };
        for (n, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            let declares = trimmed.starts_with("--ink-soft:");
            let uses = line.contains("var(--ink-soft");
            if declares || uses {
                ghosts.push(format!(
                    "{}:{} — {}",
                    entry.strip_prefix(root).unwrap_or(&entry).display(),
                    n + 1,
                    trimmed.chars().take(72).collect::<String>()
                ));
            }
        }
    }
    assert!(
        ghosts.is_empty(),
        "`--ink-soft` is the #6e6e73 grey that made the site look faint, and it was deleted rather \
         than darkened so it could not be reused. Use `--ink` for anything a reader reads, or \
         `--ink-2` for chrome:\n  {}",
        ghosts.join("\n  ")
    );
}

/// Every file under a directory, so a test can sweep the site without listing it.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found.sort();
    found
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
    // ALL of them, since v0.0.140 — so this stops being a ratchet and becomes an equality, for the
    // reason the backend-coverage test gives: a floor cannot see a regression that stays above the
    // line, and there is no line left below full.
    //
    // It got here as a floor: 8 when the sweep was added at v0.0.136, then 11 with contracts, 13
    // with the `decreases` measure, 18 with bounds and remainder, 21 with CInt range, mixed-scale
    // overflow and read_file.
    assert_eq!(
        kept, total,
        "the Burxt backend kept {} of {} runtime guarantees. It kept ALL of them from v0.0.140, \
         so this is a regression — a program compiled by stage-1 no longer enforces:\n  {}",
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
            // `css`, `js`, `html` and `yml` are here because the website is now made of them. The
            // list used to stop at `py`, which covered the generators and not one line of what they
            // generate into — so `docs/assets/site.js`, the layouts, and the sidebar's data file
            // could each have gone missing with `git status` clean, which is the precise failure
            // this test exists to prevent and the reason docs/ was invisible until v0.0.105.
            let interesting = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(
                    e,
                    "bx" | "md" | "rs" | "toml" | "json" | "sh" | "py" | "css" | "js" | "html"
                        | "yml"
                )
            });
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

/// Every call site that can pass a record mirrors the ABI attributes the callee declares.
///
/// This exists because it did not, and the result was a **wrong answer in money**. A vtable
/// target declares its record parameter `byval(T)` — on x86-64 that means the aggregate
/// travels in the stack argument area. The indirect call passed a bare pointer, which
/// travels in a register. The callee read its record from wherever the stack happened to
/// be, and a `Bool` field decided the answer from garbage: a taxable item taxed at 0.0000,
/// no crash, no warning.
///
/// `tests/pass/abi_dyn_record_params.bx` reproduces it, and that fixture is necessary but
/// NOT sufficient — the failure is stack-layout dependent, so it is caught by luck. Adding
/// a `print` to the failing program made it start answering correctly, which is exactly why
/// six earlier reductions all "passed". A test that depends on frame layout is a test that
/// will stop working without telling anyone.
///
/// So the durable guard is structural, and the shape of the defect says why: there are
/// three places that build a call passing user arguments, and **two of them** mirrored the
/// attributes. Not a hard problem — an incomplete sweep, the same failure as the thirteen
/// runtime guarantees stage-1 silently dropped. This asserts the sweep is complete: any
/// site that inspects `is_aggregate` while building arguments must also attach `byval`.
#[test]
fn every_call_site_mirrors_the_declared_abi() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = fs::read_to_string(root.join("src/codegen.rs")).unwrap();
    let lines: Vec<&str> = src.lines().collect();

    let mut sites = 0;
    let mut missing = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !(line.contains(".build_call(") || line.contains(".build_indirect_call(")) {
            continue;
        }
        // Only calls that pass USER arguments are at risk. The runtime helpers
        // (`burxt.checked.add` and friends) take i64 and can hold no aggregate, and they
        // are recognised by the absence of any `is_aggregate` test while building values.
        //
        // `return tail` is the one user call deliberately not counted: it never tests
        // `is_aggregate` because typeck refuses an aggregate there outright — *"a
        // guaranteed tail call is limited to scalar parameters and returns"*. Checked, not
        // assumed. If that restriction is ever lifted, this sweep will start counting the
        // site and demand the attributes, which is the behaviour we want.
        let before = lines[i.saturating_sub(60)..i].join("\n");
        if !before.contains("is_aggregate") {
            continue;
        }
        sites += 1;
        let after = lines[i..(i + 45).min(lines.len())].join("\n");
        if !after.contains("\"byval\"") {
            missing.push(format!("  src/codegen.rs:{} — {}", i + 1, line.trim()));
        }
    }

    assert!(
        missing.is_empty(),
        "these call sites pass records but never attach `byval`, so the callee reads its \
         argument from the wrong place — a wrong answer, not a crash:\n{}",
        missing.join("\n")
    );
    // A floor, so deleting the mirroring by deleting the call site is not a way to pass.
    assert!(
        sites >= 3,
        "expected at least 3 call sites passing records (direct call, direct method, \
         vtable); found {}. If a site was removed, say so here — this number is the \
         evidence that the sweep is complete.",
        sites
    );
}


/// `burxt review` reports what changed about what a program PROMISES — and, just as importantly,
/// stays silent when nothing did.
///
/// The tool exists because the most dangerous change in agent-written code is a weakened contract:
/// an agent that cannot satisfy `requires amount <= self.balance` deletes it, which passes every
/// test — the tests were failing BECAUSE of it. No other language can flag that, because
/// everywhere else the assertion is a line in a body.
///
/// Two of these four fixtures exist for failures already made while building it:
///
/// * `contract_deleted` — the first version compared `Contract.text`, and `Parser::new` keeps no
///   source, so every text came back EMPTY and two empty strings compared equal. The tool reported
///   the privacy and `pure` weakenings and silently missed the one it exists for. A tool that
///   under-reports is worse than no tool, because it is believed.
/// * `reformatted` — comparing text then reported a renamed parameter as a lost contract AND a
///   gained one: two WEAKENED lines for a change that weakened nothing. A tool that cries wolf on
///   a rename teaches a reviewer to skim past the line that mattered. Clauses are now compared
///   structurally, with parameters rendered by position, and shown as the programmer wrote them.
///
/// The exit code is part of the contract: non-zero exactly when something was weakened, so this
/// works as a CI gate without parsing the output.
#[test]
fn review_reports_weakened_promises_and_nothing_else() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/review");
    let mut checked = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".expect") else { continue };
        let expect = fs::read_to_string(&path).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("review")
            .arg(dir.join(format!("{}.old.bx", stem)))
            .arg(dir.join(format!("{}.new.bx", stem)))
            .output()
            .expect("burxt review");
        let shown = String::from_utf8_lossy(&out.stdout);
        for wanted in expect.lines().filter(|l| !l.trim().is_empty()) {
            assert!(
                shown.contains(wanted),
                "review of `{}` did not report {:?}\nit said:\n{}",
                stem,
                wanted,
                shown
            );
        }
        // Exit 1 exactly when a weakening was reported, so CI can gate on it directly.
        let weakened = shown.contains("WEAKENED");
        let code = out.status.code().unwrap_or(-1);
        assert_eq!(
            code,
            i32::from(weakened),
            "review of `{}` exited {} but {} a weakening — the exit code is what a CI gate reads",
            stem,
            code,
            if weakened { "reported" } else { "reported no" }
        );
        checked += 1;
    }
    assert!(checked >= 4, "expected at least four review fixtures, ran {}", checked);
}

/// `examples/refused/README.md` says exactly what the compiler says.
///
/// That page IS the argument this language makes: it shows a reviewer ten mistakes an agent writes
/// confidently, and what the compiler says instead. So a made-up message there would be a lie
/// about the one thing being sold — and the guide already told that lie once, with two invented
/// error messages, which running the examples is what caught.
///
/// This regenerates the page and diffs it, exactly as `the_site_examples_are_not_stale` does for
/// the website. It also means IMPROVING a message is a two-line change rather than a hunt: fix the
/// compiler, rerun the script, and the page follows.
#[test]
fn the_refusals_page_is_not_stale() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("scripts/refused.py");
    let out = Command::new("python3")
        .arg(&script)
        .arg("--check")
        .env("BURXT", env!("CARGO_BIN_EXE_burxt"))
        .current_dir(root)
        .output()
        .expect("python3 scripts/refused.py --check");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Every panel must actually be refused. A panel that starts compiling — because a rule was
    // relaxed or a fixture drifted — would silently become an advertisement for something the
    // language no longer does, which is worse than having no page.
    let page = fs::read_to_string(root.join("examples/refused/README.md")).unwrap();
    assert!(
        !page.contains("ACCEPTED"),
        "a panel in examples/refused/ now COMPILES. Either the refusal was lost, or the example \
         needs rewriting to still demonstrate one:\n{}",
        page.lines().filter(|l| l.contains("ACCEPTED")).collect::<Vec<_>>().join("\n")
    );
    let panels = page.matches("\n## ").count();
    assert!(panels >= 10, "examples/refused/ lost panels: {} left", panels);
}

/// Every `burxt` code block on the landing page compiles, and the one that claims an answer
/// produces it.
///
/// The front page is the first thing anyone reads and the last thing anyone re-reads, which is
/// exactly how it rots. This project has already shipped a guide containing two INVENTED error
/// messages and a function that did not exist; both were caught by running the examples, not by
/// proofreading.
///
/// So the rule is the same one `scripts/site-examples.py` and `scripts/refused.py` follow: if a
/// page shows code, a test compiles it. Fragments are skipped — a three-line excerpt with no
/// declaration in sight is not wrong, it is partial — which is decided by whether the block
/// contains a `function`, a `class` or a `let`.
#[test]
fn the_landing_page_code_compiles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let page = fs::read_to_string(root.join("docs/index.md")).unwrap();
    let scratch = scratch_dir("landing-page");
    fs::create_dir_all(&scratch).unwrap();

    let mut checked = 0;
    let mut failures = Vec::new();

    // The HERO sample is HTML, not a fenced block — it lives inside the `hero` div where a
    // markdown fence does not render, so it is written as `<pre><code>` with `<` and `>` escaped.
    // It is also the most-read code on the site, which makes it the most important to compile:
    // the first version of this test looked only at fences and quietly checked everything except
    // the one line a visitor actually reads.
    let mut blocks: Vec<String> = Vec::new();
    if let Some((_, rest)) = page.split_once("<pre><code>") {
        if let Some((body, _)) = rest.split_once("</code></pre>") {
            blocks.push(body.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"));
        }
    }
    blocks.extend(
        page.split("```burxt")
            .skip(1)
            .filter_map(|b| b.split_once("```").map(|(body, _)| body.trim_start_matches('\n').to_string())),
    );

    for (i, source) in blocks.into_iter().enumerate() {
        // Only whole declarations, not excerpts.
        if !(source.contains("function ") || source.contains("class ") || source.contains("let ")) {
            continue;
        }
        let path = scratch.join(format!("block{}.bx", i));
        fs::write(&path, &source).unwrap();
        let out = burxt("check", &path, &scratch);
        if !out.status.success() {
            failures.push(format!(
                "docs/index.md block {} does not compile:\n{}\n{}",
                i + 1,
                source,
                String::from_utf8_lossy(&out.stdout)
            ));
        }
        checked += 1;
    }

    // The page claims `59.97`. Claiming an answer is a stronger promise than claiming it compiles,
    // so it is checked separately — and by running the program, not by trusting the comment.
    let sample = scratch.join("money.bx");
    fs::write(
        &sample,
        "let price: Decimal<2> = 19.99;\nlet quantity: Int     = 3;\nlet total: Decimal<2> = price * quantity;\nprint(total);\n",
    )
    .unwrap();
    let ran = burxt("run", &sample, &scratch);
    let shown = String::from_utf8_lossy(&ran.stdout);
    let printed: String = shown.lines().filter(|l| !l.starts_with("compiled ")).collect();
    if printed.trim() != "59.97" {
        failures.push(format!(
            "docs/index.md says that program prints 59.97; it printed {:?}",
            printed.trim()
        ));
    }

    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(checked >= 3, "expected at least three code blocks on the landing page, found {}", checked);
}

/// Every whole-declaration `burxt` block in the guide compiles.
///
/// The guide has lied twice. It quoted two error messages the compiler never produced, and it
/// referred to a `parse_int` that did not exist — both caught by running the examples, neither by
/// proofreading. Prose does not fail to compile, so a page can go on teaching something false
/// indefinitely while every other test stays green.
///
/// Fragments are skipped on purpose, and the distinction matters: a three-line excerpt with no
/// declaration in sight is *partial*, not wrong, and wrapping it in a guessed context would fail
/// for reasons the guide is not at fault for. A block earns checking when it contains a
/// `function`, a `class`, an `interface` or an `enum` — that is, when it is a thing the compiler
/// can be asked about on its own.
#[test]
fn the_guide_code_compiles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("guide-code");
    fs::create_dir_all(&scratch).unwrap();
    // `use "lib/..."` resolves relative to the file, so the library travels with the snippets.
    let _ = std::os::unix::fs::symlink(root.join("lib"), scratch.join("lib"));

    let mut checked = 0;
    let mut failures = Vec::new();
    let mut pages: Vec<PathBuf> = fs::read_dir(root.join("docs/guide"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    pages.sort();

    for page in &pages {
        let text = fs::read_to_string(page).unwrap();
        let name = page.file_stem().unwrap().to_string_lossy().into_owned();
        // `reference.md` is a table of FORMS, not a narrative: its snippets name types that exist
        // nowhere on the page because their job is to show a shape — `function largest<T: Ordered>`
        // illustrates a bound, not a program. The numbered pages are the ones that teach with code
        // a reader can run, and those are what this test is for.
        if name == "reference" || name == "index" || name == "README" {
            continue;
        }
        let mut declarations: Vec<String> = Vec::new();
        let mut every_block: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (i, block) in text.split("```burxt").skip(1).enumerate() {
            let Some((source, after)) = block.split_once("```") else { continue };
            let source = source.trim_start_matches('\n');
            // A block the page shows as REFUSED — its error quoted immediately after — is expected
            // to fail, and checked for failing. That is stronger than skipping it: the guide's
            // refusal examples are the ones most likely to rot, because a rule can be relaxed
            // years after the page was written.
            //
            // "Immediately" is load-bearing. Without a distance limit this matched page 3, where a
            // perfectly good `class Account` is followed by prose and THEN an error about a
            // different snippet — so the test declared a compiling example broken. The gap must be
            // short: a lead-in like "From outside:" and nothing more.
            let refusal = after
                .split_once("```")
                .map(|(gap, rest)| {
                    gap.trim().len() < 60 && rest.trim_start().starts_with("error:")
                })
                .unwrap_or(false);
            let whole = ["function ", "class ", "interface ", "enum "]
                .iter()
                .any(|k| source.contains(k));
            if !whole {
                continue;
            }
            // `{ ... }` and `-> ...` are elisions a reader understands and a compiler cannot.
            if source.contains("...") {
                continue;
            }
            // Unbalanced braces mean an EXCERPT — page 4 shows a single signature line ending in
            // `{` to make a point about one word in it. A reader completes that mentally; a
            // compiler cannot, and demanding they be whole would push the guide toward padding
            // every illustration into a runnable program.
            if source.matches('{').count() != source.matches('}').count() {
                continue;
            }
            // A signature LISTING — `function` lines with no body at all — is a reference table.
            // Page 11 lists the entire Map API that way, which brace-balancing cannot spot because
            // there are no braces to balance.
            if source.contains("function ") && !source.contains('{') {
                continue;
            }
            // A block showing SEVERAL files — `// a.bx` then `// b.bx` — cannot be one program by
            // nature. Page 8 is about modules, so most of its examples are this shape.
            if source.contains(".bx\n") && source.trim_start().starts_with("//") {
                continue;
            }
            // Compiled with every earlier whole block on the same page in front of it, because
            // that is how a reader meets it: page 3 declares `class Book` in one block and
            // implements an interface for it three blocks later. Checking each block alone
            // reported three failures that were not the guide being wrong — they were the guide
            // being a page rather than a list of programs.
            // Accepted if it compiles EITHER on its own or with the page's earlier blocks in
            // front of it. Both readings are legitimate and a page uses both: page 3 declares
            // `class Book` once and implements an interface for it three blocks later (needs the
            // prefix), and also shows `class Account` twice as it builds the idea up (needs to be
            // read alone, or the second one is a redeclaration).
            // Three readings, and the block passes if ANY of them compiles: alone, with the
            // page's earlier type declarations, or with every earlier block. A guide page is a
            // narrative — it declares a class in one block and adds methods four blocks later, and
            // it also shows the same class twice as it builds an idea up. Insisting on one
            // assembly produced failures that were artifacts of the assembly rather than defects
            // in the page, which is a test lying about its subject.
            let readings = [
                source.to_string(),
                format!("{}\n{}", declarations.join("\n"), source),
                format!("{}\n{}", every_block.join("\n"), source),
            ];
            let mut out = None;
            for reading in &readings {
                // Imports must come first in a Burxt file, so every `use` is hoisted above the
                // declarations — otherwise assembling two blocks puts one in the middle.
                let mut program = String::new();
                let mut body = String::new();
                if !reading.contains("use \"") {
                    if reading.contains("Option<") || reading.contains("Option.") {
                        program.push_str("use \"lib/option.bx\";\n");
                    }
                    if reading.contains("Result<") || reading.contains("Result.") {
                        program.push_str("use \"lib/result.bx\";\n");
                    }
                }
                for line in reading.lines() {
                    if line.trim_start().starts_with("use \"") {
                        if !program.contains(line.trim()) {
                            program.push_str(line.trim());
                            program.push('\n');
                        }
                    } else {
                        body.push_str(line);
                        body.push('\n');
                    }
                }
                program.push_str(&body);
                let path = scratch.join(format!("{}-{}.bx", name, i));
                fs::write(&path, &program).unwrap();
                let attempt = burxt("check", &path, &scratch);
                let ok = attempt.status.success();
                if out.is_none() || ok {
                    out = Some(attempt);
                }
                if ok {
                    break;
                }
            }
            let out = out.unwrap();

            every_block.push(source.to_string());
            for line in source.lines() {
                for keyword in ["class ", "interface ", "enum "] {
                    if let Some(rest) = line.trim_start().strip_prefix(keyword) {
                        let named = rest
                            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                            .next()
                            .unwrap_or("");
                        if !named.is_empty() && seen.insert(named.to_string()) {
                            declarations.push(source.to_string());
                        }
                    }
                }
            }
            if refusal {
                if out.status.success() {
                    failures.push(format!(
                        "docs/guide/{}.md block {} is shown as REFUSED but now compiles. Either \
                         the rule was relaxed, or the example needs rewriting to still \
                         demonstrate one:\n{}",
                        name,
                        i + 1,
                        source
                    ));
                }
            } else if !out.status.success() {
                failures.push(format!(
                    "docs/guide/{}.md block {} does not compile:\n{}\n{}",
                    name,
                    i + 1,
                    source,
                    String::from_utf8_lossy(&out.stdout)
                ));
            }
            checked += 1;
        }
    }

    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(checked >= 15, "expected the guide to hold at least fifteen whole examples, found {}", checked);
}

/// The guide reads in order: every numbered page's heading, its `title:`, and its `Next` link agree
/// with its filename.
///
/// Renumbering the guide in v0.0.161 to insert the effects page renamed four files and left all
/// four H1s behind, so `07-ffi.md` opened with "# 6. The C boundary" and `10-absence-and-failure.md`
/// with "# 9.". A reader following the guide in order met 6 twice, then 7 twice, and never saw an
/// 11 at all. `the_guide_code_compiles` cannot see this — every code block on those pages was
/// perfectly fine — and neither can a link checker, because every link resolved.
///
/// It is the shape of rot worth a test: a rename is mechanical, the thing it breaks is prose, and
/// nothing else in the suite reads prose.
#[test]
fn the_guide_reads_in_order() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pages: Vec<(u32, String, String)> = fs::read_dir(root.join("docs/guide"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .filter_map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            // `01-` … `11-`. reference.md, index.md and README.md carry no number by design.
            let n: u32 = name.split('-').next()?.parse().ok()?;
            Some((n, name, fs::read_to_string(&p).unwrap()))
        })
        .collect();
    pages.sort();

    let mut problems = Vec::new();
    assert!(pages.len() >= 11, "expected at least eleven numbered guide pages, found {}", pages.len());

    for (i, (n, name, text)) in pages.iter().enumerate() {
        assert_eq!(
            *n as usize,
            i + 1,
            "guide page numbers must run 1..n with no gap; found {} at position {}",
            name,
            i + 1
        );

        // The H1 states the page's own number.
        match text.lines().find(|l| l.starts_with("# ")) {
            Some(h1) => {
                let stated = h1
                    .trim_start_matches("# ")
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if stated != n.to_string() {
                    problems.push(format!(
                        "docs/guide/{} opens with `{}` — the heading says {} and the filename says {}",
                        name, h1, stated, n
                    ));
                }
            }
            None => problems.push(format!("docs/guide/{} has no `# ` heading", name)),
        }

        // Every page but the last hands off to the next one by filename, so following the guide
        // forward cannot skip a page or loop back to one already read.
        if let Some((_, next_name, _)) = pages.get(i + 1) {
            let link = next_name.trim_end_matches(".md");
            if !text.contains(&format!("({}.md", link)) {
                problems.push(format!(
                    "docs/guide/{} never links to {} — a reader following `Next` would stop here",
                    name, next_name
                ));
            }
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A bracket contract and the `requires` it desugars to produce **byte-identical** failure messages.
///
/// spec/M13-CONTRACT-SYNTAX.md opens by claiming exactly this, and says the desugaring is
/// "observable rather than asserted". It was neither: the bracket form shipped in v0.0.135 with no
/// fixture anywhere in the suite, and `src/parser.rs` carried a comment citing a
/// `tests/pass/contract_brackets.bx` that had never existed. Fourteen versions of a syntax nobody
/// tested — found in v0.0.166 while deciding how stage-1 should render the message.
///
/// Not a fixture pair, because that is the wrong instrument here: a `.stderr` file pins ONE text and
/// what matters is that TWO programs agree. Comparing the two runs directly says the thing the spec
/// says, so a divergence fails rather than needing somebody to notice two files drifting.
///
/// It also pins the answer to the question the spec spent two versions on. `[> $0.00]` on `balance`
/// reports `balance > $0.00` — the SUBJECT, synthesized in — and not the written fragment, because a
/// message that does not name the value that broke sends the reader back to the declaration to find
/// out. Andre's call, and the reason is the one the language is organised around.
#[test]
fn bracket_contracts_desugar_to_the_same_message() {
    let scratch = scratch_dir("bracket-desugar");
    fs::create_dir_all(&scratch).unwrap();

    // Each pair: the same constraint written as a bracket and as a clause, and the message both
    // must produce. The parameter form, the elided-subject return form, and a clause naming a
    // SECOND parameter — which is the case a synthesized subject could most easily get wrong.
    let cases: [(&str, &str, &str); 6] = [
        (
            "function withdraw(balance: Decimal<2> [> $0.00], amount: Decimal<2>) -> Decimal<2> {\n\
             return balance - amount;\n\
             }\n\
             print(withdraw($0.00, $3.00));\n",
            "function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>\n\
             requires balance > $0.00\n\
             {\n\
             return balance - amount;\n\
             }\n\
             print(withdraw($0.00, $3.00));\n",
            "`requires balance > $0.00` failed in `withdraw`",
        ),
        (
            "function fee(amount: Decimal<2>) -> Decimal<2> [>= $0.00] {\n\
             return amount;\n\
             }\n\
             print(fee(-$1.00));\n",
            "function fee(amount: Decimal<2>) -> Decimal<2>\n\
             ensures result >= $0.00\n\
             {\n\
             return amount;\n\
             }\n\
             print(fee(-$1.00));\n",
            "`ensures result >= $0.00` failed in `fee`",
        ),
        (
            "function take(balance: Decimal<2>, amount: Decimal<2> [<= balance]) -> Decimal<2> {\n\
             return balance - amount;\n\
             }\n\
             print(take($1.00, $9.00));\n",
            "function take(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>\n\
             requires amount <= balance\n\
             {\n\
             return balance - amount;\n\
             }\n\
             print(take($1.00, $9.00));\n",
            "`requires amount <= balance` failed in `take`",
        ),
        // `it` names the subject where elision cannot reach — spec Decision 2. The message resolves
        // it too: reporting `it > $0.00 || it > -$100.00` would name no value, which is the tax the
        // synthesized-subject decision was taken to avoid.
        (
            "function band(balance: Decimal<2> [it > $0.00 || it > -$100.00]) -> Decimal<2> {\n\
             return balance;\n\
             }\n\
             print(band(-$500.00));\n",
            "function band(balance: Decimal<2>) -> Decimal<2>\n\
             requires balance > $0.00 || balance > -$100.00\n\
             {\n\
             return balance;\n\
             }\n\
             print(band(-$500.00));\n",
            "`requires balance > $0.00 || balance > -$100.00` failed in `band`",
        ),
        // `it` on a RETURN bracket is `result`, and TWO clauses using it — the case where computing
        // "did this clause use it" as a change across the clause reported the second one unresolved,
        // because the flag it read was monotonic.
        (
            "function bounded(amount: Decimal<2>) -> Decimal<2> [it >= $0.00, it < $100.00] {\n\
             return amount;\n\
             }\n\
             print(bounded($500.00));\n",
            "function bounded(amount: Decimal<2>) -> Decimal<2>\n\
             ensures result >= $0.00\n\
             ensures result < $100.00\n\
             {\n\
             return amount;\n\
             }\n\
             print(bounded($500.00));\n",
            "`ensures result < $100.00` failed in `bounded`",
        ),
        // The comma is AND and `||` is OR, so a bracket is a LIST of claims each of which may be any
        // Bool expression. Two clauses here, not three — and the failing one is quoted with its
        // parentheses intact, which is the whole reason the comma exists rather than one `&&`.
        (
            "function banded(v: Decimal<2> [it > $0.00, (it < $10.00 || it > $2000.00)]) \
             -> Decimal<2> {\n\
             return v;\n\
             }\n\
             print(banded($50.00));\n",
            "function banded(v: Decimal<2>) -> Decimal<2>\n\
             requires v > $0.00\n\
             requires (v < $10.00 || v > $2000.00)\n\
             {\n\
             return v;\n\
             }\n\
             print(banded($50.00));\n",
            "`requires (v < $10.00 || v > $2000.00)` failed in `banded`",
        ),
    ];

    for (i, (bracketed, written, expected)) in cases.iter().enumerate() {
        let mut messages = Vec::new();
        for (spelling, source) in [("bracket", bracketed), ("clause", written)] {
            let path = scratch.join(format!("{}-{}.bx", spelling, i));
            fs::write(&path, source).unwrap();
            let out = burxt("run", &path, &scratch);
            assert!(
                !out.status.success(),
                "the {} spelling of case {} was supposed to FAIL its contract:\n{}{}",
                spelling,
                i,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            // Without dropping the `compiled <path> -> <out>` line the two runs differ by their
            // own filenames, which is the comparison saying something true and useless.
            let stderr: String = String::from_utf8_lossy(&out.stderr)
                .lines()
                .filter(|l| !l.starts_with("compiled "))
                .map(|l| format!("{}\n", l))
                .collect();
            assert!(
                stderr.contains(expected),
                "the {} spelling of case {} reported the wrong clause\n  expected to contain: \
                 {:?}\n  actual: {:?}",
                spelling,
                i,
                expected,
                stderr
            );
            messages.push(stderr);
        }
        assert_eq!(
            messages[0], messages[1],
            "case {}: the bracket form and the `requires` form reported DIFFERENT messages, so the \
             desugaring is observable and wrong",
            i
        );
    }

    let _ = fs::remove_dir_all(&scratch);
}

/// `burxt mcp-schema` derives the tool schema from the PRECONDITIONS — so changing a contract changes
/// the schema, and forgetting to update one of the two is not a thing that can happen.
///
/// The second half is the test. Anyone can generate a schema; the claim is that it **cannot drift**,
/// and the only way to check that is to change a contract and watch the schema follow. A test that
/// merely compared the output to a recorded string would pass forever while the derivation quietly
/// stopped reading the clauses at all.
///
/// This is also the invariant that would have caught the M13 bracket form going fourteen versions with
/// no fixture: the thing being claimed is a RELATIONSHIP between two artifacts, and a relationship
/// needs both sides varied.
#[test]
fn the_mcp_schema_follows_the_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("mcp-schema");
    fs::create_dir_all(&scratch).unwrap();

    let schema_of = |source: &str, name: &str| -> String {
        let path = scratch.join(format!("{}.bx", name));
        fs::write(&path, source).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("mcp-schema")
            .arg(&path)
            .current_dir(&scratch)
            .output()
            .expect("burxt mcp-schema");
        assert!(
            out.status.success(),
            "mcp-schema failed on {}:\n{}",
            name,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // The bracket form and the written form are the SAME sentence, so they must produce the same
    // schema down to the byte. Clauses are read structurally rather than as text, which is what makes
    // that true rather than a coincidence that holds today.
    let bracketed = schema_of(
        "function line_total(unit: Decimal<2> [> $0.00], quantity: Int [> 0, <= 100000])\n\
         -> Decimal<2> { return unit * quantity; }\n",
        "bracketed",
    );
    let written = schema_of(
        "function line_total(unit: Decimal<2>, quantity: Int) -> Decimal<2>\n\
         requires unit > $0.00\n\
         requires quantity > 0\n\
         requires quantity <= 100000\n\
         { return unit * quantity; }\n",
        "written",
    );
    assert_eq!(
        bracketed, written,
        "the bracket form and the `requires` form produced DIFFERENT schema, so one of them is not \
         being read as a contract"
    );

    // Every bound arrived, and as EXACT DIGITS. `0.00` never went through a float — a `DecimalLit` is
    // already an unscaled integer and a scale, so rendering it inserts a point.
    for needle in [
        "\"exclusiveMinimum\":\"0.00\"",
        "\"exclusiveMinimum\":\"0\"",
        "\"maximum\":\"100000\"",
        "\"unit\"",
        "\"quantity\"",
    ] {
        assert!(
            bracketed.contains(needle),
            "the schema is missing {}:\n{}",
            needle,
            bracketed
        );
    }

    // THE ANTI-DRIFT CHECK. Tighten one clause and the schema must move with it.
    let tightened = schema_of(
        "function line_total(unit: Decimal<2> [>= $1.00], quantity: Int [> 0, <= 50])\n\
         -> Decimal<2> { return unit * quantity; }\n",
        "tightened",
    );
    assert_ne!(
        bracketed, tightened,
        "changing two preconditions did not change the schema, so the schema is not derived from them"
    );
    assert!(
        tightened.contains("\"minimum\":\"1.00\"") && tightened.contains("\"maximum\":\"50\""),
        "the tightened bounds did not reach the schema:\n{}",
        tightened
    );
    assert!(
        !tightened.contains("100000"),
        "the OLD bound survived a change to the contract, which is the drift this tool exists to \
         prevent:\n{}",
        tightened
    );

    // Only the file's OWN functions are tools. Without this filter, a three-line server published the
    // entire standard library — `string_find`, `file_delete`, `os_run` — because the loader
    // concatenates. Inviting a model to call `os_run` because it happened to be in scope is the exact
    // shape of failure this project is against.
    let _ = std::os::unix::fs::symlink(root.join("lib"), scratch.join("lib"));
    let with_imports = schema_of(
        "use \"lib/string.bx\";\n\
         function priced(unit: Decimal<2> [> $0.00]) -> Decimal<2> { return unit; }\n",
        "with_imports",
    );
    assert!(
        with_imports.contains("\"priced\""),
        "the file's own function is missing:\n{}",
        with_imports
    );
    assert!(
        !with_imports.contains("string_find") && !with_imports.contains("string_split"),
        "a `use`d module's functions were published as tools:\n{}",
        with_imports
    );

    // A clause JSON Schema cannot express is LEFT OUT and reported, never approximated. `amount <=
    // balance` relates two parameters and has no key; emitting something for it would be the drift.
    let relational = scratch.join("relational.bx");
    fs::write(
        &relational,
        "function withdraw(balance: Decimal<2>, amount: Decimal<2> [<= balance]) -> Decimal<2>\n\
         { return balance - amount; }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("mcp-schema")
        .arg(&relational)
        .current_dir(&scratch)
        .output()
        .expect("burxt mcp-schema");
    let note = String::from_utf8_lossy(&out.stderr);
    assert!(
        note.contains("could not be expressed"),
        "a relational clause was silently dropped instead of reported:\n{}",
        note
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// The MCP server answers a real `initialize` / `tools/list` / `tools/call` exchange.
///
/// Against a recorded transcript, because "it runs" is not the claim. The claim is that money crosses
/// the wire with all its digits — `19.99 * 3` comes back as `59.97` and not `59.96999999999999` — and
/// that a request violating a precondition gets a JSON-RPC error rather than taking the process down.
///
/// That last part is why the server checks arguments itself as well as declaring contracts on the
/// tools. A server must not die on a bad request, so the polite check has to exist; and if the polite
/// check and the contract ever disagreed, the contract would abort LOUDLY rather than let a bad value
/// through. The redundancy is a tripwire on what would otherwise be a silent divergence.
#[test]
fn the_mcp_server_answers_a_real_exchange() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("mcp-server");
    fs::create_dir_all(&scratch).unwrap();
    let server = scratch.join("server");

    let built = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/mcp/server.bx"))
        .arg("-o")
        .arg(&server)
        .current_dir(&scratch)
        .output()
        .expect("build the server");
    assert!(
        built.status.success(),
        "the MCP server did not build:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );

    let requests = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":"19.99","quantity":3}}}"#, "\n",
        // The same amount sent as a JSON NUMBER rather than a string. Both are read, because an exact
        // producer sends a string and a careless one sends a number, and the difference carries no
        // information about what was meant.
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":19.99,"quantity":3}}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"tax_on","arguments":{"subtotal":"59.97","rate":"0.0825"}}}"#, "\n",
        // A precondition violated: an error, and the process survives to answer the next one.
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":"0.00","quantity":3}}}"#, "\n",
        // More precision than Decimal<2> holds. Refused, never rounded — the caller sent a third
        // decimal place for a reason and no default here can know what it was.
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":"19.999","quantity":1}}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#, "\n",
        "not json at all\n",
        // And it kept going after every one of those.
        r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"line_total","arguments":{"unit":"1.00","quantity":2}}}"#, "\n",
    );

    let mut child = Command::new(&server)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .current_dir(&scratch)
        .spawn()
        .expect("spawn the server");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(requests.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("the server exited");
    assert!(out.status.success(), "the server exited {:?}", out.status.code());
    let lines: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(lines.len(), 9, "expected nine responses, got:\n{}", lines.join("\n"));

    let expect = |i: usize, needle: &str| {
        assert!(
            lines[i].contains(needle),
            "response {} is missing {:?}:\n{}",
            i + 1,
            needle,
            lines[i]
        );
    };
    expect(0, "\"protocolVersion\":\"2024-11-05\"");
    // 19.99 x 3 = 59.97. Exactly, from a string and from a JSON number alike.
    expect(1, "\"text\":\"59.97\"");
    expect(2, "\"text\":\"59.97\"");
    // 59.97 x 8.25% = 4.947525, half-to-even at two places.
    expect(3, "\"text\":\"4.95\"");
    expect(4, "\"code\":-32602");
    expect(5, "\"code\":-32602");
    expect(6, "\"code\":-32601");
    expect(7, "\"code\":-32700");
    expect(8, "\"text\":\"2.00\"");

    let _ = fs::remove_dir_all(&scratch);
}

/// The store, through a real file — spec/N9-VECTORS-EXACTLY.md row 4.
///
/// `tests/pass/vector_store.bx` covers the format itself and must not write into the repository, so
/// the half that actually touches the disk lives here: write a corpus, APPEND a row to it without
/// rewriting the file (which is the reason the format is one object per line), read it back, and
/// assert the scores are the same values — not close ones.
///
/// A store that loses a digit on the way to disk has exactly the wobble the arithmetic was built to
/// remove, so this is row 5's reproducibility claim applied to persistence.
#[test]
fn the_vector_store_round_trips_a_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("vector-store");
    fs::create_dir_all(&scratch).unwrap();

    let program = format!(
        r#"use "{}/lib/vector.bx";

function vec3(a: Decimal<7>, b: Decimal<7>, c: Decimal<7>) -> [Decimal<7>] {{
    let mutable v: [Decimal<7>] = [];
    let p: Int = push(v, a);
    let q: Int = push(v, b);
    let r: Int = push(v, c);
    return v;
}}

let tick: Decimal<7> = 0.0000001;
let query: [Decimal<7>] = vec3(0.0000000, 1.0000000, 0.0000000);

let mutable rows: [Row] = [];
let r1: Int = push(rows, Row {{ id: "east", values: vec3(1.0000000, 0.0000000, 0.0000000) }});
let r2: Int = push(rows, Row {{ id: "mostly-north", values: vec3(0.6000000, 0.8000000, 0.0000000) }});

let written: Int = vector_store_write("corpus.jsonl", rows);
// Appended, not rewritten. This is what JSONL buys.
let added: Int = vector_store_append("corpus.jsonl", Row {{
    id: "down",
    values: vec3(tick * (0 - 6000000), 0.8000000, tick * (0 - 10000000)),
}});

match vector_store_read("corpus.jsonl") {{
    Error(why) => {{ print("refused: " + why); }}
    Ok(back) => {{
        print(len(back));
        let found: [Scored] = vector_top_dot(back, query, 3);
        let mutable k: Int = 0;
        while k < len(found) {{
            print(back[found[k].at].id + " " + to_string(found[k].score));
            k += 1;
        }}
    }}
}}
"#,
        root.display()
    );
    let source = scratch.join("store.bx");
    fs::write(&source, program).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("run")
        .arg(&source)
        .current_dir(&scratch)
        .output()
        .expect("run the store program");
    assert!(
        out.status.success(),
        "the store program failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Three rows: two written, one appended.
    //
    // Every score is asserted as a NUMBER and not a range. `east` is orthogonal to the query and
    // still ranks, at exactly zero; `down` scores 0.8 on the y component alone. These are the same
    // digits a float store would give as 0.79999995 on one machine and 0.80000001 on another.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![
            "3",
            "mostly-north 0.80000000000000",
            "down 0.80000000000000",
            "east 0.00000000000000",
        ],
        "the corpus did not survive the file:\n{}",
        stdout
    );

    // And the file on disk is the format the spec claims, line for line.
    let written = fs::read_to_string(scratch.join("corpus.jsonl")).unwrap();
    assert_eq!(
        written,
        concat!(
            r#"{"id":"east","values":["1.0000000","0.0000000","0.0000000"]}"#,
            "\n",
            r#"{"id":"mostly-north","values":["0.6000000","0.8000000","0.0000000"]}"#,
            "\n",
            r#"{"id":"down","values":["-0.6000000","0.8000000","-1.0000000"]}"#,
            "\n",
        ),
        "the store's text format changed"
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// A rounding contract widens at EVERY declared position, and drops at NONE — in both compilers.
///
/// This is a relationship test, and it exists because the claim was a comment for thirteen versions
/// and the comment was wrong. `storable`'s own doc said "used at every position, since v0.0.181" while
/// seven positions still compared types with `==` — including `return`, which the same comment named
/// in its list. A fixture that only exercised the accepting side would have passed throughout, because
/// the accepting side was what worked; and a fixture that only exercised the refusing side would have
/// passed too, because refusing everything refuses the wrong things as well.
///
/// So both directions are varied against the same position. A position passes only if the widened
/// program compiles AND the dropped program is refused. Anything else is a site where the rule is
/// half-implemented, which is exactly the state v0.0.181 left behind.
///
/// Both compilers, because stage-1 turned out to be AHEAD of stage-0 here: it used `fits` everywhere
/// except `push`, so the differential found this pointing the other way for once.
#[test]
fn a_contract_widens_at_every_position_and_drops_at_none() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("contract-positions");
    fs::create_dir_all(&scratch).unwrap();
    let stage1 = scratch.join("stage1");
    let have_stage1 = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(have_stage1, "stage-1 did not build");

    // Each case is a body written twice: once storing a value INTO the contracted type (must be
    // accepted) and once storing a contracted value into the plain one (must be refused). `{have}`
    // is the type of the value being stored and `{want}` the declared type it is stored into, so a
    // single template serves both directions and the two runs cannot drift apart.
    let positions: &[(&str, &str)] = &[
        ("return", "function f(p: {have}) -> {want} { return p; }\nprint(f(v));"),
        ("a parameter", "function f(p: {want}) -> Int { return 1; }\nprint(f(v));"),
        ("a let", "let b: {want} = v;\nprint(b);"),
        (
            "a field initializer",
            "class C { a: {want} }\nlet c: C = C { a: v };\nprint(c.a);",
        ),
        (
            "a field assignment",
            "class C { a: {want} }\nlet mutable c: C = C { a: w };\nc.a = v;\nprint(c.a);",
        ),
        (
            "a method argument",
            "class C { a: Int,\n  function (self) t(x: {want}) -> Int { return 1; } }\n\
             let c: C = C { a: 1 };\nprint(c.t(v));",
        ),
        (
            "a growable array literal",
            "let mutable g: [{want}] = [v];\nprint(g[0]);",
        ),
        (
            "push",
            "let mutable g: [{want}] = [];\nlet n: Int = push(g, v);\nprint(len(g));",
        ),
        (
            "a fixed array literal",
            "let f: [{want}; 2] = [v, v];\nprint(f[0]);",
        ),
        (
            "an index assignment",
            "let mutable f: [{want}; 2] = [w, w];\nf[0] = v;\nprint(f[0]);",
        ),
    ];

    const PLAIN: &str = "Decimal<7>";
    const CARRIED: &str = "Decimal<7, RoundHalfEven>";

    let mut wrong = Vec::new();
    for (name, template) in positions {
        // Widening: the value has no contract, the declared type has one. Must compile.
        // Dropping: the reverse. Must be refused.
        for (widening, have, want) in
            [(true, PLAIN, CARRIED), (false, CARRIED, PLAIN)]
        {
            let program = format!(
                "let v: {have} = 0.6000000;\nlet w: {want} = 0.6000000;\n{}\n",
                template.replace("{have}", have).replace("{want}", want),
            );
            let source = scratch.join("case.bx");
            fs::write(&source, &program).unwrap();

            let rust_ok = Command::new(env!("CARGO_BIN_EXE_burxt"))
                .arg("check")
                .arg(&source)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            // stage-1 reports a count rather than an exit code.
            let emitted = Command::new(&stage1)
                .arg(&source)
                .arg(scratch.join("case.ll"))
                .output()
                .expect("stage-1");
            let said = String::from_utf8_lossy(&emitted.stdout).to_string();
            let burxt_ok = said.contains("type errors: 0");

            for (compiler, ok) in [("stage-0", rust_ok), ("stage-1", burxt_ok)] {
                if ok != widening {
                    wrong.push(format!(
                        "{} at {}: {} the {} program\n{}",
                        compiler,
                        name,
                        if ok { "ACCEPTED" } else { "refused" },
                        if widening { "widening" } else { "dropping" },
                        program,
                    ));
                }
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "the rule is half-implemented at {} site(s):\n\n{}",
        wrong.len(),
        wrong.join("\n")
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// The one Burxt program used by both cross-compilation invariants below. Money math, because that is
/// the thing whose answer must not depend on the machine.
const CROSS_PROGRAM: &str = "\
function tax(subtotal: Decimal<2>, rate: Decimal<4, RoundHalfEven>) -> Decimal<2, RoundHalfEven> {
    return subtotal * rate;
}
function total(unit: Decimal<2>, quantity: Int) -> Decimal<2> {
    return unit * quantity;
}
let subtotal: Decimal<2> = total($19.99, 3);
print(subtotal);
print(tax(subtotal, 0.0825));
";

/// `--target <triple>` emits a real object file for that architecture — spec/FAR-HORIZON-ROADMAP M3.
///
/// The architecture is read out of the object's own header rather than trusted, and rather than shelled
/// out to `file(1)`: what is being checked is that LLVM was handed the triple and acted on it, and a
/// test that only checked the exit status would pass while every target silently emitted host code.
///
/// Four container formats on purpose — ELF, Mach-O, COFF and wasm — because those are Linux, macOS,
/// Windows and the web, which are the three reach targets plus the one nobody expected to be free.
#[test]
fn cross_compilation_emits_a_real_object_for_every_target() {
    let scratch = scratch_dir("cross-object");
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("money.bx");
    fs::write(&source, CROSS_PROGRAM).unwrap();

    // (triple, what its object header must say)
    enum Shape {
        /// ELF: magic, then e_machine as a little-endian u16 at offset 18.
        Elf(u16),
        /// Mach-O 64-bit little-endian: magic feedfacf, then cputype at offset 4.
        MachO(u32),
        /// COFF: machine as a little-endian u16 at offset 0.
        Coff(u16),
        /// WebAssembly: `\0asm` and version 1.
        Wasm,
    }
    let targets: &[(&str, Shape)] = &[
        ("aarch64-unknown-linux-gnu", Shape::Elf(183)),
        ("x86_64-unknown-linux-gnu", Shape::Elf(62)),
        ("riscv64-unknown-linux-gnu", Shape::Elf(243)),
        ("armv7-unknown-linux-gnueabihf", Shape::Elf(40)),
        ("x86_64-apple-darwin", Shape::MachO(0x0100_0007)),
        ("aarch64-apple-darwin", Shape::MachO(0x0100_000C)),
        ("x86_64-pc-windows-msvc", Shape::Coff(0x8664)),
        ("wasm32-unknown-unknown", Shape::Wasm),
    ];

    let mut wrong = Vec::new();
    for (triple, shape) in targets {
        let obj = scratch.join(format!("{}.o", triple));
        let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .args(["build"])
            .arg(&source)
            .args(["--target", triple, "-o"])
            .arg(&obj)
            .output()
            .expect("burxt");
        if !out.status.success() {
            wrong.push(format!(
                "{}: did not build\n{}",
                triple,
                String::from_utf8_lossy(&out.stderr)
            ));
            continue;
        }
        let bytes = match fs::read(&obj) {
            Ok(b) => b,
            Err(e) => {
                wrong.push(format!("{}: no object written ({})", triple, e));
                continue;
            }
        };
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let u32_at = |i: usize| {
            u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]])
        };
        let verdict = match shape {
            Shape::Elf(machine) => {
                if bytes.len() < 20 || &bytes[0..4] != b"\x7fELF" {
                    Err("not an ELF file".to_string())
                } else if u16_at(18) != *machine {
                    Err(format!("ELF e_machine is {}, wanted {}", u16_at(18), machine))
                } else {
                    Ok(())
                }
            }
            Shape::MachO(cputype) => {
                if bytes.len() < 8 || u32_at(0) != 0xfeed_facf {
                    Err("not a 64-bit Mach-O file".to_string())
                } else if u32_at(4) != *cputype {
                    Err(format!("Mach-O cputype is {:#x}, wanted {:#x}", u32_at(4), cputype))
                } else {
                    Ok(())
                }
            }
            Shape::Coff(machine) => {
                if bytes.len() < 2 || u16_at(0) != *machine {
                    Err(format!("COFF machine is {:#x}, wanted {:#x}", u16_at(0), machine))
                } else {
                    Ok(())
                }
            }
            Shape::Wasm => {
                if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
                    Err("not a WebAssembly module".to_string())
                } else {
                    Ok(())
                }
            }
        };
        if let Err(why) = verdict {
            wrong.push(format!("{}: {}", triple, why));
        }

        // And it must NOT have linked: linking a foreign object needs that target's libc and
        // linker, and spec/M3's decision is to delegate that rather than own it. The message has to
        // say so, or a caller reads "compiled" and looks for an executable that is not there.
        let said = String::from_utf8_lossy(&out.stderr);
        if !said.contains("not linked") {
            wrong.push(format!("{}: did not say the object is unlinked:\n{}", triple, said));
        }
    }

    // `run` builds for THIS machine, so it cannot honour a triple — and saying so is better than
    // building something and then failing to execute it.
    let ran = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .args(["run"])
        .arg(&source)
        .args(["--target", "aarch64-unknown-linux-gnu"])
        .output()
        .expect("burxt");
    if ran.status.success() {
        wrong.push("`run --target` was accepted; it cannot be".to_string());
    }

    // A triple LLVM has no backend for is named, with where to look. The old code called
    // `initialize_native`, so EVERY foreign triple failed with "no available targets are
    // compatible" — a message about the compiler's own initialisation rather than about the input.
    let bad = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .args(["build"])
        .arg(&source)
        .args(["--target", "sparc9-unknown-nonesuch", "-o"])
        .arg(scratch.join("bad.o"))
        .output()
        .expect("burxt");
    let complaint = String::from_utf8_lossy(&bad.stderr);
    if bad.status.success() || !complaint.contains("no backend for target") {
        wrong.push(format!("an unknown triple was not named: {}", complaint));
    }

    assert!(
        wrong.is_empty(),
        "cross-compilation is wrong for {} target(s):\n\n{}",
        wrong.len(),
        wrong.join("\n")
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// **The IR is IDENTICAL for every target, apart from the two lines that name the target.**
///
/// This is the cross-target claim in spec/FAR-HORIZON-ROADMAP M3 — "the same money math, provably
/// identical on web, desktop and mobile" — turned into something that can fail. It holds for a reason
/// worth stating rather than a coincidence:
///
/// - **No float.** Every arithmetic operation is on an i64, so nothing depends on a CPU's rounding
///   mode, x87 excess precision, or fused-multiply-add. This is the whole no-float thesis paying a
///   dividend nobody designed it for.
/// - **Layout is decided by TYPE, never by size.** An enum's payload area is counted in 8-byte cells
///   by the type of its variants, so it does not change with a pointer's width.
/// - **Opaque pointers.** LLVM 15+ writes `ptr`, not `i8*`, so even pointer WIDTH never appears in the
///   IR — which is why wasm32 and ARM32 are in the list below beside the 64-bit targets. That one was
///   not predicted; the roadmap expected 64-bit agreement only.
///
/// What this does and does not prove: identical IR means nothing in the *arithmetic* can diverge, so
/// a Decimal answer is the same everywhere. It does not prove identical *behaviour* — LLVM's own
/// lowering and the platform's libc are still downstream. But that surface is very much smaller than
/// float rounding, and it is the surface every language has.
#[test]
fn the_ir_is_the_same_for_every_target() {
    let scratch = scratch_dir("cross-ir");
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("money.bx");
    fs::write(&source, CROSS_PROGRAM).unwrap();

    let ir_for = |triple: Option<&str>| -> String {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_burxt"));
        cmd.arg("emit-ir").arg(&source);
        if let Some(t) = triple {
            cmd.args(["--target", t]);
        }
        let out = cmd.output().expect("burxt");
        assert!(
            out.status.success(),
            "emit-ir failed for {:?}:\n{}",
            triple,
            String::from_utf8_lossy(&out.stderr)
        );
        // The two lines that are SUPPOSED to differ are the two being dropped.
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.starts_with("target triple") && !l.starts_with("target datalayout"))
            .map(|l| format!("{}\n", l))
            .collect()
    };

    let host = ir_for(None);
    assert!(host.contains("define"), "the host IR is empty:\n{}", host);

    let mut differ = Vec::new();
    for triple in [
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "riscv64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
        // Both 32-bit, and both identical — see the note above.
        "armv7-unknown-linux-gnueabihf",
        "wasm32-unknown-unknown",
    ] {
        let there = ir_for(Some(triple));
        if there != host {
            let first = there
                .lines()
                .zip(host.lines())
                .position(|(a, b)| a != b)
                .map(|i| {
                    format!(
                        "line {}:\n  {} says: {}\n  host says: {}",
                        i + 1,
                        triple,
                        there.lines().nth(i).unwrap_or(""),
                        host.lines().nth(i).unwrap_or("")
                    )
                })
                .unwrap_or_else(|| "the IR is a different length".to_string());
            differ.push(format!("{}\n{}", triple, first));
        }
    }

    assert!(
        differ.is_empty(),
        "the IR is NOT target-independent for {} target(s), so the exact-arithmetic-everywhere \
         claim is no longer true:\n\n{}",
        differ.len(),
        differ.join("\n\n")
    );

    // And the two dropped lines really do change, or the comparison above proves nothing: it would
    // pass just as well if --target were ignored entirely.
    let stamped = |triple: &str| -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("emit-ir")
            .arg(&source)
            .args(["--target", triple])
            .output()
            .expect("burxt");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| l.starts_with("target "))
            .map(|l| format!("{}\n", l))
            .collect()
    };
    let arm = stamped("aarch64-unknown-linux-gnu");
    let win = stamped("x86_64-pc-windows-msvc");
    assert!(
        arm.contains("aarch64-unknown-linux-gnu"),
        "--target did not reach the module:\n{}",
        arm
    );
    assert_ne!(
        arm, win,
        "two different triples produced the same target lines, so --target is being ignored"
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// A program reports its status to the shell, and the SAME status from both compilers.
///
/// Not a `tests/pass/` fixture, and the reason is structural: that harness compares stdout and
/// requires success, so a program whose whole point is exiting 3 cannot live there. Which is also
/// why this went unnoticed — nothing in the suite could express it.
///
/// The audit's row read: *"a CLI that cannot signal failure to a shell is not shippable."* It could
/// not, because `external function exit` is refused — the runtime owns that symbol, since a contract
/// failure is what calls it — so a Burxt program had no way to say it failed. v0.0.200 makes `exit` a
/// statement.
#[test]
fn a_program_reports_its_status_to_the_shell() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("exit-status");
    fs::create_dir_all(&scratch).unwrap();

    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    let stage1 = scratch.join("stage1");
    let have_stage1 = llc.exists()
        && Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("build")
            .arg(root.join("examples/stage1.bx"))
            .arg("-o")
            .arg(&stage1)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

    // (program, the status a shell must see)
    let cases: &[(&str, i32)] = &[
        // Failure, reported. This is the case that was impossible.
        ("print(\"failing\");\nexit(3);\n", 3),
        // Success, said explicitly.
        ("print(\"fine\");\nexit(0);\n", 0),
        // Falling off the end is still success — `exit` adds a way to say something, it does not
        // change what silence means.
        ("print(\"fine\");\n", 0),
        // The boundary values of a status.
        ("exit(255);\n", 255),
        ("exit(1);\n", 1),
        // Inside a function, and after a branch, because that is where a real CLI puts it.
        (
            "function main_or_die(ok: Bool) -> Int {\n\
               if !ok { exit(4); }\n\
               return 0;\n\
             }\n\
             print(main_or_die(false));\n",
            4,
        ),
        // A status the checker cannot fold: 0..=255 is enforced at runtime, and that exits 70 —
        // the runtime's own failure code — rather than the out-of-range status.
        (
            "function status(n: Int) -> Int { return n * 100; }\nexit(status(3));\n",
            70,
        ),
    ];

    let mut wrong = Vec::new();
    for (i, (program, want)) in cases.iter().enumerate() {
        let source = scratch.join(format!("case{}.bx", i));
        fs::write(&source, program).unwrap();

        // ---- stage-0 ----
        let exe = scratch.join(format!("case{}", i));
        let built = Command::new(env!("CARGO_BIN_EXE_burxt"))
            .arg("build")
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("burxt");
        if !built.status.success() {
            wrong.push(format!(
                "case {}: did not build\n{}",
                i,
                String::from_utf8_lossy(&built.stderr)
            ));
            continue;
        }
        let ran = Command::new(&exe).output().expect("the program");
        if ran.status.code() != Some(*want) {
            wrong.push(format!(
                "case {}: stage-0 exited {:?}, wanted {}\n{}",
                i,
                ran.status.code(),
                want,
                program
            ));
        }

        // ---- stage-1, the same program, the same status ----
        if !have_stage1 {
            continue;
        }
        let ll = scratch.join(format!("case{}.ll", i));
        let emitted = Command::new(&stage1).arg(&source).arg(&ll).output().expect("stage-1");
        if !String::from_utf8_lossy(&emitted.stdout).contains("bytes of IR") {
            wrong.push(format!("case {}: stage-1 refused it\n{}", i, program));
            continue;
        }
        let obj = scratch.join(format!("case{}.o", i));
        let assembled = Command::new(llc)
            .args(["-relocation-model=pic", "-filetype=obj", "-o"])
            .arg(&obj)
            .arg(&ll)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let s1exe = scratch.join(format!("case{}-s1", i));
        if !assembled
            || !Command::new("cc")
                .arg(&obj)
                .args(["-o"])
                .arg(&s1exe)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        {
            wrong.push(format!("case {}: stage-1's object did not link", i));
            continue;
        }
        let s1ran = Command::new(&s1exe).output().expect("the stage-1 program");
        if s1ran.status.code() != Some(*want) {
            wrong.push(format!(
                "case {}: stage-1 exited {:?}, wanted {} — the two compilers disagree about a \
                 status, which a shell can see\n{}",
                i,
                s1ran.status.code(),
                want,
                program
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} program(s) reported the wrong status:\n\n{}",
        wrong.len(),
        wrong.join("\n")
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// `print_error` writes to stderr, and `print` does not — in both compilers, byte for byte.
///
/// A named invariant rather than a `tests/pass/` fixture, because that harness compares stdout only.
/// `tests/pass/print_error.bx` covers the stdout half (that nothing leaks into it); the interesting
/// half is that everything arrives on the OTHER stream, correctly formatted, and nothing in the suite
/// could express that before v0.0.203.
///
/// Why both streams have to be read separately: the first build of this had stage-1's Decimal path
/// writing its DIGITS with its own `printf` and only the newline through the shared helper — so
/// `print_error($19.99)` put `19.99` on stdout and the newline on stderr. A test that concatenated the
/// two streams would have passed.
#[test]
fn print_error_writes_to_stderr() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("print-error");
    fs::create_dir_all(&scratch).unwrap();
    let source = root.join("tests/pass/print_error.bx");

    // Every type the formatter knows, so a stream that is right for Ints and wrong for Decimals
    // cannot hide.
    let want_out = "one\ntwo\nthree\n";
    let want_err = "this must not appear in stdout\n42\n19.99\ntrue\ninterpolated 7\n";

    // ---- stage-0 ----
    let exe = scratch.join("pe");
    let built = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("burxt");
    assert!(
        built.status.success(),
        "print_error.bx did not build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let ran = Command::new(&exe).output().expect("the program");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        want_out,
        "stage-0 wrote the wrong thing to STDOUT — something printed with `print_error` leaked into it"
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stderr),
        want_err,
        "stage-0 wrote the wrong thing to STDERR"
    );

    // ---- stage-1, the same program, the same two streams ----
    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    if !llc.exists() {
        eprintln!("skipping the stage-1 half: {} is not installed", llc.display());
        let _ = fs::remove_dir_all(&scratch);
        return;
    }
    let stage1 = scratch.join("stage1");
    assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .expect("burxt")
        .success());
    let ll = scratch.join("pe.ll");
    let emitted = Command::new(&stage1).arg(&source).arg(&ll).output().expect("stage-1");
    assert!(
        String::from_utf8_lossy(&emitted.stdout).contains("bytes of IR"),
        "stage-1 refused print_error.bx:\n{}",
        String::from_utf8_lossy(&emitted.stdout)
    );
    let obj = scratch.join("pe.o");
    assert!(Command::new(llc)
        .args(["-relocation-model=pic", "-filetype=obj", "-o"])
        .arg(&obj)
        .arg(&ll)
        .status()
        .map(|s| s.success())
        .unwrap_or(false));
    let s1exe = scratch.join("pe-s1");
    assert!(Command::new("cc")
        .arg(&obj)
        .args(["-o"])
        .arg(&s1exe)
        .status()
        .map(|s| s.success())
        .unwrap_or(false));
    let s1ran = Command::new(&s1exe).output().expect("the stage-1 program");
    assert_eq!(
        String::from_utf8_lossy(&s1ran.stdout),
        want_out,
        "stage-1 wrote the wrong thing to STDOUT"
    );
    assert_eq!(
        String::from_utf8_lossy(&s1ran.stderr),
        want_err,
        "stage-1 wrote the wrong thing to STDERR — the two compilers disagree about which stream a \
         value goes to, or about how it is formatted on the way"
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// lib/test.bx reports a failure the way a test library has to: named, valued, on stderr, and with a
/// non-zero status so a build actually fails.
///
/// The failing path is what a test library is judged on, and it cannot live in `tests/pass/` — that
/// harness requires success and compares stdout only. So the suite lives in `tests/support/` and is
/// run here, with both streams and the exit code read separately.
///
/// Both compilers, because a test library that reported differently under the two would undermine the
/// thing it exists to establish.
#[test]
fn the_test_library_reports_failures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = scratch_dir("test-library");
    fs::create_dir_all(&scratch).unwrap();
    let source = root.join("tests/support/failing_suite.bx");

    // Seven checks, six of them wrong. Every message names WHAT was expected and what arrived —
    // "expected 5, got 4" is most of the value of a failing test, and it is why the checks are
    // per-type rather than one generic that cannot print a bare `T`.
    let want_out = "deliberately broken: 7 checks, 6 failed\n";
    let want_err = "\
FAIL deliberately broken / wrong money: expected 20.00, got 19.99
FAIL deliberately broken / wrong int: expected 5, got 4
FAIL deliberately broken / wrong text: expected \"want\", got \"got\"
FAIL deliberately broken / wrong bool: expected false, got true
FAIL deliberately broken / a claim that fails: expected this to hold, and it did not
FAIL deliberately broken / my own: because I said so
";

    let check = |label: &str, out: &std::process::Output| {
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            want_out,
            "{}: the summary is wrong",
            label
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            want_err,
            "{}: the failure report is wrong",
            label
        );
        // The status is the point: without it a failing suite cannot fail a build, which is what
        // `exit(code)` was added for in v0.0.200.
        assert_eq!(
            out.status.code(),
            Some(1),
            "{}: a suite with failures must exit 1, not {:?}",
            label,
            out.status.code()
        );
    };

    // ---- stage-0 ----
    let exe = scratch.join("suite");
    let built = Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("burxt");
    assert!(
        built.status.success(),
        "the failing suite did not build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    check("stage-0", &Command::new(&exe).output().expect("the suite"));

    // ---- stage-1 ----
    let llc = Path::new("/usr/lib/llvm-18/bin/llc");
    if !llc.exists() {
        eprintln!("skipping the stage-1 half: {} is not installed", llc.display());
        let _ = fs::remove_dir_all(&scratch);
        return;
    }
    let stage1 = scratch.join("stage1");
    assert!(Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg("build")
        .arg(root.join("examples/stage1.bx"))
        .arg("-o")
        .arg(&stage1)
        .status()
        .expect("burxt")
        .success());
    let ll = scratch.join("suite.ll");
    let emitted = Command::new(&stage1).arg(&source).arg(&ll).output().expect("stage-1");
    assert!(
        String::from_utf8_lossy(&emitted.stdout).contains("bytes of IR"),
        "stage-1 refused the failing suite:\n{}",
        String::from_utf8_lossy(&emitted.stdout)
    );
    let obj = scratch.join("suite.o");
    assert!(Command::new(llc)
        .args(["-relocation-model=pic", "-filetype=obj", "-o"])
        .arg(&obj)
        .arg(&ll)
        .status()
        .map(|s| s.success())
        .unwrap_or(false));
    let s1exe = scratch.join("suite-s1");
    assert!(Command::new("cc")
        .arg(&obj)
        .args(["-o"])
        .arg(&s1exe)
        .status()
        .map(|s| s.success())
        .unwrap_or(false));
    check("stage-1", &Command::new(&s1exe).output().expect("the stage-1 suite"));

    let _ = fs::remove_dir_all(&scratch);
}
