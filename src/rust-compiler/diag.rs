//! Where a problem is, not just what it is.
//!
//! Burxt's errors have always been sentences a person can act on. What they
//! lacked was a *position*, which is fine in a terminal and useless to an
//! editor: an editor needs a line, a column and a length to underline. This
//! module is that missing half — a byte range, plus the two ways to present it
//! (a caret rendering for humans, JSON for tools).
//!
//! Spans are byte offsets, not line/column pairs. The lexer knows offsets for
//! free; lines and columns are a *presentation* concern, computed once at the
//! edge by `LineIndex`. Storing them everywhere instead would mean every layer
//! agreeing on how to count a tab.

/// A half-open byte range in the source: `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start: start as u32, end: end as u32 }
    }
}

/// One problem, with its message and where it is.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { message: message.into(), span }
    }
}

/// A 1-based line and column, and the line's text — everything needed to point
/// at something. Columns count CHARACTERS, not bytes, so a multi-byte character
/// does not push the caret off target.
pub struct Location<'a> {
    pub line: usize,
    pub col: usize,
    pub line_text: &'a str,
}

/// Byte offset -> line/column, computed once per file.
pub struct LineIndex<'a> {
    src: &'a str,
    /// byte offset of the start of each line
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.char_indices().filter(|(_, c)| *c == '\n').map(|(i, _)| i + 1));
        LineIndex { src, starts }
    }

    /// Clamp a byte offset into the source AND onto a character boundary.
    ///
    /// **This exists because the renderer used to panic.** Every slice below is by byte
    /// offset, and Rust refuses a slice that splits a character — so a span whose end
    /// landed mid-character turned `let é: Int = ;` into a Rust backtrace and exit 101.
    /// `lexer.rs` was producing that span (`start + 1` on a two-byte character, fixed in
    /// the same version), but fixing the producer is not enough and should not be trusted
    /// to be: **a diagnostic renderer that crashes destroys the diagnostic it was called
    /// to deliver.** It is the last thing standing between a problem and the person who
    /// has to fix it, so it is the wrong place to be strict. Every offset that reaches a
    /// slice goes through here.
    ///
    /// Found by writing `src/burxt-compiler/diag.bx` — the Burxt counterpart counts bytes
    /// and is total, so it rendered this input correctly while the Rust one aborted. That
    /// is the differential test working in the direction nobody expects.
    fn boundary(&self, offset: usize) -> usize {
        let mut at = offset.min(self.src.len());
        while at > 0 && !self.src.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    pub fn locate(&self, offset: u32) -> Location<'a> {
        let mut offset = self.boundary(offset as usize);
        // An error AT the end of a file that ends with a newline would otherwise
        // be reported on the empty line after the last one — true, and useless.
        // Point at the end of the last line with content instead, which is what
        // "unexpected end of file" means to a reader.
        if offset == self.src.len() && self.src.ends_with('\n') {
            offset -= 1;
        }
        // The last line whose start is <= offset.
        let line_ix = match self.starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let start = self.starts[line_ix];
        let end = self.starts.get(line_ix + 1).map(|e| e - 1).unwrap_or(self.src.len());
        let line_text = &self.src[start..end.max(start)];
        let col = self.src[start..offset].chars().count() + 1;
        Location { line: line_ix + 1, col, line_text }
    }

    /// An LSP position (0-based line and character) back to a byte offset — the
    /// inverse of `locate`, needed to answer a question asked at a cursor.
    pub fn offset_of(&self, line: usize, character: usize) -> u32 {
        let start = match self.starts.get(line) {
            Some(s) => self.boundary(*s),
            None => return self.src.len() as u32,
        };
        let rest = &self.src[start..];
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let mut offset = start;
        for (i, c) in rest[..line_end].chars().enumerate() {
            if i == character {
                return offset as u32;
            }
            offset += c.len_utf8();
        }
        offset as u32
    }

    /// How many characters the span covers on its first line — the width of the
    /// underline. At least 1, so a zero-width span still points somewhere.
    pub fn width(&self, span: Span) -> usize {
        let start = self.boundary(span.start as usize);
        let end = self.boundary(span.end as usize).max(start);
        let line_end = self.src[start..].find('\n').map(|i| start + i).unwrap_or(self.src.len());
        self.src[start..end.min(line_end)].chars().count().max(1)
    }
}

/// Render a diagnostic the way a compiler should: the message, the location, and
/// the offending line with the span underlined.
///
/// ```text
/// error: expected `;`, found `let`
///  --> money.bx:12:19
///    |
/// 12 |     let x: Int = 1
///    |                   ^
/// ```
pub fn render(path: &str, src: &str, d: &Diagnostic) -> String {
    let index = LineIndex::new(src);
    let loc = index.locate(d.span.start);
    let width = index.width(d.span);
    let num = loc.line.to_string();
    // Same shape as every compiler worth imitating: the line number in a gutter,
    // the source echoed once, the span underlined beneath it.
    let gutter = " ".repeat(num.len());
    // Tabs in the source would misalign a caret counted in characters, so they
    // are shown as single spaces in the echoed line.
    let shown: String = loc.line_text.replace('\t', " ");
    let pad: String = shown
        .chars()
        .take(loc.col.saturating_sub(1))
        .map(|c| if c.is_whitespace() { c } else { ' ' })
        .collect();
    format!(
        "error: {msg}\n\
         {gutter}--> {path}:{line}:{col}\n\
         {gutter} |\n\
         {num} | {shown}\n\
         {gutter} | {pad}{carets}\n",
        msg = d.message,
        gutter = gutter,
        path = path,
        line = loc.line,
        col = loc.col,
        num = num,
        shown = shown,
        pad = pad,
        carets = "^".repeat(width),
    )
}

/// The same diagnostic as one line of JSON, for editors and CI.
///
/// Positions are given twice on purpose: 1-based line/column for humans and
/// terminals, and 0-based for the Language Server Protocol, which is what will
/// consume this next. Converting in the consumer is where off-by-ones live.
pub fn to_json(path: &str, src: &str, d: &Diagnostic) -> String {
    let index = LineIndex::new(src);
    let start = index.locate(d.span.start);
    let end = index.locate(d.span.end);
    format!(
        "{{\"file\":{},\"severity\":\"error\",\"message\":{},\
         \"line\":{},\"column\":{},\"endLine\":{},\"endColumn\":{},\
         \"lspStart\":{{\"line\":{},\"character\":{}}},\
         \"lspEnd\":{{\"line\":{},\"character\":{}}},\
         \"byteStart\":{},\"byteEnd\":{}}}",
        json_string(path),
        json_string(&d.message),
        start.line,
        start.col,
        end.line,
        end.col,
        start.line - 1,
        start.col - 1,
        end.line - 1,
        end.col - 1,
        d.span.start,
        d.span.end,
    )
}

/// Quote and escape a string for JSON. Hand-written because the compiler has one
/// dependency and this is not worth a second one.
pub fn json_string(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_offsets_on_the_right_line_and_column() {
        let src = "let a: Int = 1;\nlet b: Int = 2;\n";
        let ix = LineIndex::new(src);
        let first = ix.locate(0);
        assert_eq!((first.line, first.col), (1, 1));
        assert_eq!(first.line_text, "let a: Int = 1;");
        // start of line 2
        let second = ix.locate(16);
        assert_eq!((second.line, second.col), (2, 1));
        assert_eq!(second.line_text, "let b: Int = 2;");
        // The very end of a file that ends with a newline: reported on the last
        // line with CONTENT, not on the empty line after it. "Unexpected end of
        // file" pointing at a blank line is technically true and useless.
        let last = ix.locate(src.len() as u32);
        assert_eq!(last.line, 2);
        assert_eq!(last.line_text, "let b: Int = 2;");
    }

    /// Columns count characters, not bytes. A `é` earlier on the line must not
    /// push the caret one place right of what the reader sees.
    #[test]
    fn columns_count_characters_not_bytes() {
        let src = "let café: Int = 1;\n";
        let ix = LineIndex::new(src);
        let at_colon = src.find(':').unwrap() as u32;
        let loc = ix.locate(at_colon);
        assert_eq!(loc.col, 9, "`é` is two bytes but one column");
    }

    #[test]
    fn underline_is_never_zero_width_and_never_leaves_the_line() {
        let src = "let a: Int = 1;\nlet b: Int = 2;\n";
        let ix = LineIndex::new(src);
        assert_eq!(ix.width(Span { start: 0, end: 0 }), 1, "a point still points");
        assert_eq!(ix.width(Span { start: 0, end: 15 }), 15);
        // A span running past the newline is clipped to the first line.
        assert_eq!(ix.width(Span { start: 0, end: 31 }), 15);
    }

    #[test]
    fn rendering_puts_the_caret_under_the_span() {
        let src = "let a: Int = 1;\n    let b: Bool = 2;\n";
        let start = src.find("let b").unwrap();
        let d = Diagnostic::new("nope", Span::new(start, start + "let b: Bool = 2;".len()));
        let out = render("t.bx", src, &d);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "error: nope");
        assert_eq!(lines[1], " --> t.bx:2:5");
        assert_eq!(lines[3], "2 |     let b: Bool = 2;");
        assert_eq!(lines[4], "  |     ^^^^^^^^^^^^^^^^");
        // The caret run must start exactly under the span's first character.
        let code_col = lines[3].find("let b").unwrap();
        let caret_col = lines[4].find('^').unwrap();
        assert_eq!(code_col, caret_col);
    }

    /// `locate` and `offset_of` must be inverses, including past a multi-byte
    /// character — a cursor is given in characters and spans are in bytes.
    #[test]
    fn positions_round_trip_through_offsets() {
        let src = "let café: Int = 1;\nlet b: Int = 2;\n";
        let ix = LineIndex::new(src);
        for (line, ch) in [(0usize, 0usize), (0, 4), (0, 9), (1, 0), (1, 7)] {
            let offset = ix.offset_of(line, ch);
            let back = ix.locate(offset);
            assert_eq!((back.line - 1, back.col - 1), (line, ch), "at {:?}", (line, ch));
        }
        // The `é` is two bytes, so a character offset past it is not a byte offset.
        assert_eq!(ix.offset_of(0, 9), 10);
    }

    #[test]
    fn json_escapes_what_json_must_escape() {
        assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(json_string("line\nbreak"), "\"line\\nbreak\"");
        assert_eq!(json_string("tab\there"), "\"tab\\there\"");
        // A control character becomes a \u escape rather than a raw byte.
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
        // Burxt error messages are full of backticks and quotes; they survive.
        let msg = "expected `;`, found \"x\"";
        assert!(json_string(msg).contains("\\\"x\\\""));
    }

    #[test]
    fn json_gives_both_one_based_and_lsp_positions() {
        let src = "let a: Int = 1;\nlet b: Bool = 2;\n";
        let start = src.find("let b").unwrap();
        let d = Diagnostic::new("nope", Span::new(start, start + 3));
        let json = to_json("t.bx", src, &d);
        assert!(json.contains("\"line\":2"), "{}", json);
        assert!(json.contains("\"column\":1"), "{}", json);
        assert!(json.contains("\"lspStart\":{\"line\":1,\"character\":0}"), "{}", json);
    }
}
