//! `burxt lsp` — a language server over stdio.
//!
//! What it does, and nothing more: it typechecks the buffer you are editing and
//! underlines the problem. That is the whole of it, deliberately. Hover and
//! go-to-definition are worth having and are not here yet; a server that showed
//! hovers while staying silent about errors would have the priorities backwards.
//!
//! Design notes worth keeping:
//!
//! - **The buffer, not the file.** Diagnostics run on the client's in-memory
//!   text, so they are right while the file on disk is stale — which is the
//!   entire point of an editor integration.
//! - **One diagnostic at a time, honestly.** The compiler stops at the first
//!   error, so the server publishes one or none rather than pretending to a list.
//!   Recovering to report several is a compiler change, not a server change.
//! - **Publishing an empty array matters as much as publishing an error**: it is
//!   what clears the squiggle when the code becomes valid.
//! - **No panics on bad input.** A malformed message is answered or ignored, never
//!   fatal: a language server that dies takes the editor's language support with
//!   it until a restart.

use crate::diag::{Diagnostic, LineIndex};
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

/// Typecheck the buffer and publish what came back — one diagnostic, or none.
fn publish(output: &mut impl Write, uri: &str, text: &str) -> Result<(), String> {
    let diagnostics = match check_source(text) {
        Ok(()) => Vec::new(),
        Err(d) => vec![as_lsp_diagnostic(text, &d)],
    };
    send(output, diagnostics_message(uri, diagnostics))
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
fn check_source(text: &str) -> Result<(), Diagnostic> {
    let tokens = crate::lexer::Lexer::new(text).tokenize()?;
    let program = crate::parser::Parser::new(tokens).parse()?;
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

    #[test]
    fn a_valid_buffer_yields_no_diagnostics() {
        assert!(check_source("let a: Int = 1;\nprint(a);\n").is_ok());
    }

    #[test]
    fn a_broken_buffer_yields_a_positioned_diagnostic() {
        let src = "let a: Int = 1;\nlet b: Bool = 2;\n";
        let d = check_source(src).unwrap_err();
        let v = as_lsp_diagnostic(src, &d);
        // Line 2 of the file is line 1 to the protocol.
        assert_eq!(v.path(&["range", "start", "line"]), Some(&Value::num(1)));
        assert_eq!(v.path(&["range", "start", "character"]), Some(&Value::num(0)));
        assert_eq!(v.get("severity"), Some(&Value::num(SEVERITY_ERROR as f64)));
        assert_eq!(v.get("source").unwrap().as_str(), Some("burxt"));
        assert!(v.get("message").unwrap().as_str().unwrap().contains("declared Bool"));
    }
}
