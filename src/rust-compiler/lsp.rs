//! `burxt lsp` — a language server over stdio.
//!
//! What it does: typechecks the buffer you are editing, underlines the problem,
//! and answers "what is the type here?" on hover. Go-to-definition is not here
//! yet — it needs the compiler to keep name resolution rather than only its
//! result.
//!
//! Hover earns its place more here than in most languages: a `Decimal<2,
//! RoundHalfEven>` tells you the scale AND the rounding contract, and a value
//! whose contract you cannot see is exactly the kind of thing this language
//! exists to make visible.
//!
//! Hover works even in a file that does not compile, because the checker keeps
//! going past a failed statement — it only stops short at a *declaration* error
//! (a bad struct field, an unknown type in a signature), where continuing would
//! mean guessing what the author meant.
//!
//! Design notes worth keeping:
//!
//! - **The buffer, not the file.** Diagnostics run on the client's in-memory
//!   text, so they are right while the file on disk is stale — which is the
//!   entire point of an editor integration.
//! - **Every type error at once.** The typechecker recovers per statement, so a
//!   buffer with five mistakes underlines five places. A lexer or parser error
//!   still arrives alone: recovering a token stream is its own design question.
//! - **Publishing an empty array matters as much as publishing an error**: it is
//!   what clears the squiggle when the code becomes valid.
//! - **No panics on bad input.** A malformed message is answered or ignored, never
//!   fatal: a language server that dies takes the editor's language support with
//!   it until a restart.

use crate::ast::Type;
use crate::diag::{Diagnostic, LineIndex, Span};
use crate::json::Value;
use std::collections::HashMap;
use std::io::{BufRead, Write};

/// Severity 1 is Error in the protocol. Burxt has no warnings yet — every
/// diagnostic it can produce is a refusal to compile.
const SEVERITY_ERROR: i64 = 1;

pub fn serve() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();

    // uri -> the client's current text for it
    let mut docs: HashMap<String, String> = HashMap::new();
    let mut shutting_down = false;

    loop {
        let msg = match read_message(&mut input)? {
            Some(m) => m,
            // stdin closed: the client is gone.
            None => return Ok(()),
        };
        let msg = match crate::json::parse(&msg) {
            Ok(v) => v,
            // A message we cannot parse is not a reason to die.
            Err(_) => continue,
        };

        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
        let id = msg.get("id").cloned();

        match method.as_str() {
            "initialize" => {
                let result = Value::obj(vec![
                    (
                        "capabilities",
                        Value::obj(vec![
                            // 1 = Full: the client sends the whole document on
                            // every change. Incremental sync is an optimization
                            // that would need a text-edit applier to be correct,
                            // and correctness of the buffer is the one thing this
                            // server cannot get wrong.
                            ("textDocumentSync", Value::num(1)),
                            ("hoverProvider", Value::Bool(true)),
                        ]),
                    ),
                    (
                        "serverInfo",
                        Value::obj(vec![
                            ("name", Value::str("burxt-lsp")),
                            ("version", Value::str(env!("CARGO_PKG_VERSION"))),
                        ]),
                    ),
                ]);
                if let Some(id) = id {
                    respond(&mut output, id, result)?;
                }
            }

            "initialized" => {}

            "textDocument/didOpen" => {
                if let (Some(uri), Some(text)) = (
                    msg.path(&["params", "textDocument", "uri"]).and_then(|v| v.as_str()),
                    msg.path(&["params", "textDocument", "text"]).and_then(|v| v.as_str()),
                ) {
                    docs.insert(uri.to_string(), text.to_string());
                    publish(&mut output, uri, text)?;
                }
            }

            "textDocument/didChange" => {
                let uri = msg
                    .path(&["params", "textDocument", "uri"])
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                // Full sync: the LAST change carries the whole document.
                let text = msg
                    .path(&["params", "contentChanges"])
                    .and_then(|v| v.as_array())
                    .and_then(|c| c.last())
                    .and_then(|c| c.get("text"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                if let (Some(uri), Some(text)) = (uri, text) {
                    docs.insert(uri.clone(), text.clone());
                    publish(&mut output, &uri, &text)?;
                }
            }

            "textDocument/didSave" => {
                // The buffer is already authoritative; a save changes nothing the
                // server knows. Re-published anyway, because a client may have
                // dropped diagnostics in between.
                if let Some(uri) = msg
                    .path(&["params", "textDocument", "uri"])
                    .and_then(|v| v.as_str())
                {
                    if let Some(text) = docs.get(uri).cloned() {
                        publish(&mut output, uri, &text)?;
                    }
                }
            }

            "textDocument/didClose" => {
                if let Some(uri) = msg
                    .path(&["params", "textDocument", "uri"])
                    .and_then(|v| v.as_str())
                {
                    docs.remove(uri);
                    // Clear the squiggles for a file no longer open, or the client
                    // keeps showing errors for a buffer that is gone.
                    send(&mut output, diagnostics_message(uri, Vec::new()))?;
                }
            }

            "textDocument/hover" => {
                if let Some(id) = id {
                    let result = msg
                        .path(&["params", "textDocument", "uri"])
                        .and_then(|v| v.as_str())
                        .and_then(|uri| docs.get(uri).map(|text| (uri, text)))
                        .and_then(|(uri, text)| {
                            let line = msg.path(&["params", "position", "line"])?;
                            let ch = msg.path(&["params", "position", "character"])?;
                            hover_in_context(uri, text, number(line)?, number(ch)?)
                        })
                        .unwrap_or(Value::Null);
                    respond(&mut output, id, result)?;
                }
            }

            "shutdown" => {
                shutting_down = true;
                if let Some(id) = id {
                    respond(&mut output, id, Value::Null)?;
                }
            }

            "exit" => {
                // Per the protocol: clean exit after shutdown, error otherwise.
                return if shutting_down {
                    Ok(())
                } else {
                    Err("client sent `exit` without `shutdown`".to_string())
                };
            }

            // An unknown REQUEST must be answered or the client waits forever;
            // an unknown notification is ignored. The `id` is the difference.
            _ => {
                if let Some(id) = id {
                    let error = Value::obj(vec![
                        // -32601 is MethodNotFound.
                        ("code", Value::num(-32601)),
                        ("message", Value::str(format!("unsupported method `{}`", method))),
                    ]);
                    send(
                        &mut output,
                        Value::obj(vec![
                            ("jsonrpc", Value::str("2.0")),
                            ("id", id),
                            ("error", error),
                        ]),
                    )?;
                }
            }
        }
    }
}

/// A JSON number as a `usize`, for protocol positions.
fn number(v: &Value) -> Option<usize> {
    match v {
        Value::Num(n) if *n >= 0.0 => Some(*n as usize),
        _ => None,
    }
}

/// The type of the smallest expression under the cursor, if any.
///
/// Smallest wins because expressions nest: in `price * qty`, the cursor on `qty`
/// should say `Int`, not the type of the product it is part of.
/// Hover, with the file's PROGRAM resolved around it — the same context `publish` checks in.
///
/// **Blanking the imports is not enough, and measuring is what showed it.** Doing only that got
/// hover working on a file that imports something without USING it, and still answered nothing on
/// `src/burxt-compiler/main.bx`: with `use "ast.bx"` blanked, `Unit` and `Token` are unknown
/// types, the checker gives up early, and there are almost no expression types left to report. The
/// file that most needs hover is exactly the file where blanking is useless.
///
/// So this does what `check_in_context` does — load the program, splice the editor's buffer into it
/// so unsaved text is authoritative, collect types over the whole thing, and keep the ones whose
/// spans fall in this file. Positions come back relative to the file, because that is what the
/// editor asked about.
///
/// Falls back to the buffer alone when the file belongs to no program on disk, which is the case a
/// scratch buffer is in.
fn hover_in_context(uri: &str, text: &str, line: usize, character: usize) -> Option<Value> {
    let local = || hover(text, line, character);
    let Some(path) = path_of(uri) else { return local() };
    let (_, imports) = crate::strip_imports(text);
    let root = if imports.is_empty() {
        match program_using(&path) {
            Some(r) => r,
            None => return local(),
        }
    } else {
        path.clone()
    };
    let Ok((buffer, files)) = crate::load_program(root.to_str()?) else { return local() };
    let Ok(canonical) = std::fs::canonicalize(&path) else { return local() };
    let Some(mine) = files.iter().find(|f| {
        std::fs::canonicalize(&f.path).map(|c| c == canonical).unwrap_or(false)
    }) else {
        return local();
    };
    // The editor's text replaces what is on disk for this one file. `strip_imports` blanks the
    // `use` lines with spaces and keeps every offset, so the splice is length-exact and no span
    // needs adjusting — the same property `check_in_context` relies on.
    let (blanked, _) = crate::strip_imports(text);
    let mut whole = String::with_capacity(buffer.len() + blanked.len());
    whole.push_str(&buffer[..mine.start]);
    whole.push_str(&blanked);
    whole.push_str(&buffer[mine.start + mine.len..]);
    let start = mine.start as u32;
    let end = start + blanked.len() as u32;

    let index = LineIndex::new(&blanked);
    let offset = index.offset_of(line, character) + start;
    let types = collect_types_cached(&whole);
    let (span, ty) = types
        .into_iter()
        .filter(|(s, _)| s.start >= start && s.start <= end)
        .filter(|(s, _)| s.start <= offset && offset < s.end)
        .min_by_key(|(s, _)| s.end - s.start)?;
    let span = Span { start: span.start - start, end: span.end.min(end) - start };
    Some(hover_value(&blanked, span, &ty))
}

fn hover(text: &str, line: usize, character: usize) -> Option<Value> {
    let index = LineIndex::new(text);
    let offset = index.offset_of(line, character);
    let types = collect_types(text);
    let (span, ty) = types
        .into_iter()
        .filter(|(s, _)| s.start <= offset && offset < s.end)
        .min_by_key(|(s, _)| s.end - s.start)?;
    Some(hover_value(text, span, &ty))
}

/// The reply itself: the type in a fenced block, any note that explains it, and the range to
/// underline. Shared by both hover paths so they cannot answer differently for the same type.
fn hover_value(text: &str, span: Span, ty: &Type) -> Value {
    let index = LineIndex::new(text);
    let mut value = format!("```burxt\n{}\n```", ty);
    if let Some(note) = explain(ty) {
        value.push('\n');
        value.push_str(&note);
    }
    let start = index.locate(span.start);
    let end = index.locate(span.end);
    let position = |l: usize, c: usize| {
        Value::obj(vec![
            ("line", Value::num((l - 1) as f64)),
            ("character", Value::num((c - 1) as f64)),
        ])
    };
    Value::obj(vec![
        (
            "contents",
            Value::obj(vec![("kind", Value::str("markdown")), ("value", Value::str(value))]),
        ),
        (
            "range",
            Value::obj(vec![
                ("start", position(start.line, start.col)),
                ("end", position(end.line, end.col)),
            ]),
        ),
    ])
}

/// One sentence about what the type GUARANTEES, where that is not obvious from
/// its name. This is the part worth hovering for: a scale is visible in the type,
/// but what happens when a result does not fit that scale is the whole question.
fn explain(ty: &Type) -> Option<String> {
    match ty {
        Type::Decimal { scale, rounding: Some(r) } => Some(format!(
            "Exact decimal, {} decimal place{}. A result that needs rounding rounds \
             {}.",
            scale,
            if *scale == 1 { "" } else { "s" },
            match r {
                crate::ast::Rounding::HalfEven => "half to even (banker's rounding)",
                crate::ast::Rounding::HalfUp => "half away from zero",
            }
        )),
        Type::Decimal { scale, rounding: None } => Some(format!(
            "Exact decimal, {} decimal place{} — no rounding contract, so any \
             operation that could round is a compile error until one is declared.",
            scale,
            if *scale == 1 { "" } else { "s" }
        )),
        Type::CInt => Some("C's 32-bit `int`, at the FFI boundary only.".to_string()),
        Type::CPointer => Some(
            "An opaque pointer from C. Only `c_is_null(p)` and `c_string_at(p)` may look at it."
                .to_string(),
        ),
        Type::CDouble => {
            Some("C's `double`. A Decimal may not cross as one — it would lose \
                  exactness.".to_string())
        }
        Type::Dyn(t) => Some(format!(
            "A interface object: dispatch to whichever type implements `{}`, decided at \
             runtime.",
            t
        )),
        _ => None,
    }
}

/// Types for every expression the checker got through, even if checking then
/// failed: hover on the parts that are fine is more useful than nothing.
fn collect_types(text: &str) -> Vec<(Span, Type)> {
    // **The imports are blanked first, and without this `hover` was dead on every real
    // program.** `use` is resolved by a pre-pass, so the parser has never seen the word — a
    // `use` line reaches it as a syntax error, `parse()` returns `Err`, and this answered with
    // an empty list. Every file that imports anything therefore had NO hover at all, silently:
    // the reply was a well-formed `null`, which reads as "nothing here" rather than "this
    // feature is broken."
    //
    // `strip_imports` replaces each import with spaces rather than removing it — its own comment
    // says *"every offset after this line stays exactly where it was, which is why no span
    // anywhere needs adjusting"* — so the spans returned still index the editor's buffer.
    //
    // This is the fallback path, for a buffer that belongs to no program on disk.
    // `hover_in_context` is the real one: blanking alone leaves imported TYPES unknown, so on
    // `main.bx` it still answered nothing.
    //
    // Found by a subagent writing `src/burxt-compiler/lsp.bx`, whose server answered 38
    // positions on a file where this one answered none. Second time the second implementation
    // has audited the first, after `diag.bx` found `diag.rs` panicking in v0.0.216. `publish`
    // never had this bug, because it goes through `check_in_context` — and nothing compared the
    // two paths.
    let (blanked, _) = crate::strip_imports(text);
    collect_types_raw(&blanked)
}

/// The collector, memoised on the exact text it was given.
///
/// **One entry, because the access pattern is one buffer at a time.** `hover_in_context` resolves
/// the whole program around the file being edited — for `src/burxt-compiler/main.bx` that is about
/// 14,000 lines — and typechecks it to find the type under one cursor. Measured at ~1.5 s. Without
/// this, every hover paid that again, so holding the mouse still and jiggling it would queue
/// full compiles.
///
/// A single entry is the right size rather than a compromise: a person hovers repeatedly in the
/// file they are editing, and the key is the spliced text, so it invalidates itself the moment a
/// keystroke changes the buffer. An LRU over several documents would add a policy nobody has asked
/// for yet.
fn collect_types_cached(text: &str) -> Vec<(Span, Type)> {
    use std::cell::RefCell;
    thread_local! {
        static LAST: RefCell<Option<(String, Vec<(Span, Type)>)>> = const { RefCell::new(None) };
    }
    LAST.with(|last| {
        if let Some((seen, types)) = last.borrow().as_ref() {
            if seen == text {
                return types.clone();
            }
        }
        let types = collect_types_raw(text);
        *last.borrow_mut() = Some((text.to_string(), types.clone()));
        types
    })
}

/// The collector itself, over text whose imports are already resolved or blanked.
fn collect_types_raw(text: &str) -> Vec<(Span, Type)> {
    let Ok(tokens) = crate::lexer::Lexer::new(text).tokenize() else {
        return Vec::new();
    };
    let Ok(program) = crate::parser::Parser::with_source(tokens, text).parse() else {
        return Vec::new();
    };
    let mut checker = crate::typeck::TypeChecker::new();
    let _ = checker.check(&program);
    checker.expr_types()
}

/// Read one `Content-Length`-framed message. `None` means stdin ended.
fn read_message(input: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = input.read_line(&mut line).map_err(|e| e.to_string())?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            // Blank line ends the headers.
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            length = rest.trim().parse().ok();
        }
        // Content-Type is the only other header defined, and its value is fixed.
    }
    let length = length.ok_or("message had no Content-Length header")?;
    let mut buf = vec![0u8; length];
    input.read_exact(&mut buf).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map(Some).map_err(|e| e.to_string())
}

fn send(output: &mut impl Write, message: Value) -> Result<(), String> {
    let body = message.write();
    // Content-Length counts BYTES, not characters — a message containing a
    // non-ASCII identifier would otherwise be truncated at the client.
    write!(output, "Content-Length: {}\r\n\r\n{}", body.len(), body).map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}

fn respond(output: &mut impl Write, id: Value, result: Value) -> Result<(), String> {
    send(
        output,
        Value::obj(vec![
            ("jsonrpc", Value::str("2.0")),
            ("id", id),
            ("result", result),
        ]),
    )
}

/// Typecheck the buffer and publish what came back — every problem, or none.
fn publish(output: &mut impl Write, uri: &str, text: &str) -> Result<(), String> {
    // A file is not always a program. `src/burxt-compiler/check.bx` is one of five modules
    // another file `use`s, and checking it alone reports every type declared in a sibling
    // as unknown — five files of squiggles that are not mistakes. So: if this file is used
    // by a program, check THE PROGRAM, and keep only the diagnostics that landed here.
    if let Some(diagnostics) = check_in_context(uri, text) {
        return send(output, diagnostics_message(uri, diagnostics));
    }
    let diagnostics = check_source(text)
        .err()
        .unwrap_or_default()
        .iter()
        .map(|d| as_lsp_diagnostic(text, d))
        .collect();
    send(output, diagnostics_message(uri, diagnostics))
}

/// Check the program this file belongs to, if it belongs to one, and answer only the
/// diagnostics inside this file. `None` when the file is its own program — the ordinary
/// case, and the cheap path.
///
/// The editor's unsaved text is what the user is looking at, so it wins over what is on
/// disk: the file is written to a temporary copy of its own directory tree? No — simpler
/// and honest: the buffer is used for THIS file and the disk for the others, by loading
/// the program and splicing the buffer over this file's span. An edit that changes the
/// file's length shifts later files, which the source map already accounts for.
fn check_in_context(uri: &str, text: &str) -> Option<Vec<Value>> {
    let path = path_of(uri)?;
    // Two ways a file belongs to a program. It may BE one — a root with `use` lines, whose
    // imports have to be resolved or every `use` is a parse error, which is what the editor
    // used to show on `src/burxt-compiler/main.bx`. Or it may be one of the modules a root assembles,
    // in which case the root is what has to be checked.
    let (_, imports) = crate::strip_imports(text);
    let root = if imports.is_empty() { program_using(&path)? } else { path.clone() };
    let (buffer, files) = crate::load_program(root.to_str()?).ok()?;
    // Where this file sits in the concatenated buffer, and what the editor has for it.
    let canonical = std::fs::canonicalize(&path).ok()?;
    let mine = files.iter().find(|f| {
        std::fs::canonicalize(&f.path).map(|c| c == canonical).unwrap_or(false)
    })?;
    let (blanked, _) = crate::strip_imports(text);
    let mut whole = String::with_capacity(buffer.len() + blanked.len());
    whole.push_str(&buffer[..mine.start]);
    whole.push_str(&blanked);
    whole.push_str(&buffer[mine.start + mine.len..]);
    let start = mine.start as u32;
    let end = start + blanked.len() as u32;

    let found = check_source(&whole).err().unwrap_or_default();
    Some(
        found
            .iter()
            .filter(|d| d.span.start >= start && d.span.start <= end)
            .map(|d| {
                // Positions are relative to this file, not to the buffer the program was
                // assembled into.
                let local = Diagnostic {
                    span: crate::diag::Span {
                        start: d.span.start - start,
                        end: d.span.end.min(end) - start,
                    },
                    ..d.clone()
                };
                as_lsp_diagnostic(&blanked, &local)
            })
            .collect(),
    )
}

fn path_of(uri: &str) -> Option<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    Some(std::path::PathBuf::from(rest))
}

/// A `.bx` file whose `use` closure reaches `target`. Searched in the file's own directory
/// and its ancestors up to three levels, which covers `src/burxt-compiler/check.bx` being
/// assembled by `src/burxt-compiler/main.bx` without walking a whole disk on every keystroke.
fn program_using(target: &std::path::Path) -> Option<std::path::PathBuf> {
    let canonical = std::fs::canonicalize(target).ok()?;
    let mut dir = target.parent()?.to_path_buf();
    for _ in 0..3 {
        let mut candidates: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "bx"))
            .collect();
        candidates.sort();
        for candidate in candidates {
            if std::fs::canonicalize(&candidate).ok() == Some(canonical.clone()) {
                continue;
            }
            if let Ok((_, files)) = crate::load_program(candidate.to_str()?) {
                if files.len() > 1
                    && files.iter().any(|f| {
                        std::fs::canonicalize(&f.path).ok() == Some(canonical.clone())
                    })
                {
                    return Some(candidate);
                }
            }
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn diagnostics_message(uri: &str, diagnostics: Vec<Value>) -> Value {
    Value::obj(vec![
        ("jsonrpc", Value::str("2.0")),
        ("method", Value::str("textDocument/publishDiagnostics")),
        (
            "params",
            Value::obj(vec![
                ("uri", Value::str(uri)),
                ("diagnostics", Value::Arr(diagnostics)),
            ]),
        ),
    ])
}

/// The front end, and only the front end: no LLVM context, no object file. The
/// editor asks "is this legal?" on every keystroke, so this has to stay cheap.
fn check_source(text: &str) -> Result<(), Vec<Diagnostic>> {
    let tokens = crate::lexer::Lexer::new(text).tokenize().map_err(|d| vec![d])?;
    let program = crate::parser::Parser::with_source(tokens, text).parse().map_err(|d| vec![d])?;
    crate::typeck::TypeChecker::new().check(&program)?;
    Ok(())
}

fn as_lsp_diagnostic(src: &str, d: &Diagnostic) -> Value {
    let index = LineIndex::new(src);
    let start = index.locate(d.span.start);
    let end = index.locate(d.span.end);
    let position = |line: usize, col: usize| {
        // LSP counts from zero; `locate` counts from one, for people.
        Value::obj(vec![
            ("line", Value::num((line - 1) as f64)),
            ("character", Value::num((col - 1) as f64)),
        ])
    };
    Value::obj(vec![
        (
            "range",
            Value::obj(vec![
                ("start", position(start.line, start.col)),
                ("end", position(end.line, end.col)),
            ]),
        ),
        ("severity", Value::num(SEVERITY_ERROR as f64)),
        ("source", Value::str("burxt")),
        ("message", Value::str(&d.message)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The framing has to be exact: a client reads Content-Length bytes and stops.
    #[test]
    fn frames_messages_with_a_byte_length() {
        let mut out: Vec<u8> = Vec::new();
        send(&mut out, Value::obj(vec![("a", Value::str("é"))])).unwrap();
        let text = String::from_utf8(out).unwrap();
        let (headers, body) = text.split_once("\r\n\r\n").unwrap();
        assert_eq!(headers, format!("Content-Length: {}", body.len()));
        assert!(body.len() > body.chars().count(), "the é makes bytes exceed chars");
    }

    #[test]
    fn reads_a_framed_message_back() {
        let raw = "Content-Length: 13\r\n\r\n{\"method\":1}\n";
        let mut cursor = std::io::Cursor::new(raw.as_bytes());
        let msg = read_message(&mut cursor).unwrap().unwrap();
        assert_eq!(msg, "{\"method\":1}\n");
    }

    #[test]
    fn a_closed_input_ends_the_loop_rather_than_erroring() {
        let mut cursor = std::io::Cursor::new(b"".as_ref());
        assert!(read_message(&mut cursor).unwrap().is_none());
    }

    /// The smallest expression under the cursor wins, because expressions nest.
    #[test]
    fn hover_reports_the_innermost_type() {
        let src = "let price: Decimal<2, RoundHalfEven> = $19.99;\nlet qty: Int = 3;\nprint(price * qty);\n";
        // Line 3 (index 2): `print(price * qty);` — the cursor on `qty`.
        let at_qty = src.lines().nth(2).unwrap().find("qty").unwrap();
        let v = hover(src, 2, at_qty).expect("expected a hover on `qty`");
        let text = v.path(&["contents", "value"]).unwrap().as_str().unwrap();
        assert!(text.contains("Int"), "got {:?}", text);
        assert!(!text.contains("Decimal"), "the product's type is not `qty`'s: {:?}", text);

        // The cursor on `price` inside the same product.
        let at_price = src.lines().nth(2).unwrap().find("price").unwrap();
        let v = hover(src, 2, at_price).expect("expected a hover on `price`");
        let text = v.path(&["contents", "value"]).unwrap().as_str().unwrap();
        assert!(text.contains("Decimal<2, RoundHalfEven>"), "got {:?}", text);
        // The contract is the part worth hovering for.
        assert!(text.contains("half to even"), "got {:?}", text);
    }

    /// An inferred binding has no annotation to read, so hover is where its type
    /// lives now. This is the trade `let x = 0;` makes explicit: the type did not
    /// disappear, it moved into the editor. See spec/M10-ERGONOMICS.md §4.6.
    #[test]
    fn hover_reports_a_type_that_was_never_written() {
        let src = "region r {\n    let price = $19.99;\n    let rate = 8.25%;\n    \
                   let tax = price * rate;\n    print(tax);\n}\n";
        let at_tax = src.lines().nth(4).unwrap().find("tax").unwrap();
        let v = hover(src, 4, at_tax).expect("expected a hover on `tax`");
        let text = v.path(&["contents", "value"]).unwrap().as_str().unwrap();
        // Nowhere in the program does the word `Decimal` appear.
        assert!(!src.contains("Decimal"));
        assert!(text.contains("Decimal<6>"), "got {:?}", text);

        let at_price = src.lines().nth(1).unwrap().find("19.99").unwrap();
        let v = hover(src, 1, at_price).expect("expected a hover on the literal");
        let text = v.path(&["contents", "value"]).unwrap().as_str().unwrap();
        assert!(text.contains("Decimal<2>"), "got {:?}", text);
    }

    /// Hover works throughout a file that does not compile — above AND below the
    /// mistake — because the checker recovers per statement instead of stopping.
    /// This test used to assert the opposite; error recovery is what changed it.
    #[test]
    fn hover_answers_on_both_sides_of_an_error() {
        let src = "let a: Int = 1;\nlet b: Bool = 2;\nprint(a);\n";
        let at_one = src.lines().next().unwrap().find('1').unwrap();
        let above = hover(src, 0, at_one).expect("hover should work above the error");
        assert!(above.path(&["contents", "value"]).unwrap().as_str().unwrap().contains("Int"));

        let at_a = src.lines().nth(2).unwrap().find('a').unwrap();
        let below = hover(src, 2, at_a).expect("hover should work BELOW the error too");
        assert!(below.path(&["contents", "value"]).unwrap().as_str().unwrap().contains("Int"));
    }

    /// The server publishes EVERY problem, not the first: a buffer with three
    /// mistakes must underline three places, or the editor quietly hides two.
    #[test]
    fn every_mistake_in_the_buffer_is_published() {
        let src = "let a: Bool = 1;\nlet b: Int = 2;\nlet c: String = b;\nlet d: Int = \"x\";\n";
        let found = check_source(src).unwrap_err();
        assert_eq!(found.len(), 3, "expected three diagnostics, got {:?}", found);
        let lines: Vec<u32> = found
            .iter()
            .map(|d| LineIndex::new(src).locate(d.span.start).line as u32)
            .collect();
        assert_eq!(lines, vec![1, 3, 4], "in source order");
    }

    #[test]
    fn hover_on_whitespace_is_nothing_rather_than_a_guess() {
        let src = "let a: Int = 1;\n\nprint(a);\n";
        assert!(hover(src, 1, 0).is_none());
    }

    #[test]
    fn a_valid_buffer_yields_no_diagnostics() {
        assert!(check_source("let a: Int = 1;\nprint(a);\n").is_ok());
    }

    #[test]
    fn a_broken_buffer_yields_a_positioned_diagnostic() {
        let src = "let a: Int = 1;\nlet b: Bool = 2;\n";
        let found = check_source(src).unwrap_err();
        assert_eq!(found.len(), 1, "one mistake, one diagnostic");
        let v = as_lsp_diagnostic(src, &found[0]);
        // Line 2 of the file is line 1 to the protocol, and the range covers the
        // offending VALUE (`2` at character 14) rather than the whole binding —
        // the declaration is not what is wrong.
        assert_eq!(v.path(&["range", "start", "line"]), Some(&Value::num(1)));
        assert_eq!(v.path(&["range", "start", "character"]), Some(&Value::num(14)));
        assert_eq!(v.get("severity"), Some(&Value::num(SEVERITY_ERROR as f64)));
        assert_eq!(v.get("source").unwrap().as_str(), Some("burxt"));
        assert!(v.get("message").unwrap().as_str().unwrap().contains("declared Bool"));
    }
}
