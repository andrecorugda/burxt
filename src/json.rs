//! A small, correct JSON reader and writer.
//!
//! Written rather than depended on, for two reasons. The compiler has exactly one
//! dependency (LLVM) and that restraint is worth keeping. And the alternative
//! people reach for at this size — finding fields with string search — is wrong
//! the moment a document contains a quote or a backslash, which Burxt source does
//! constantly. A language server that mangles the buffer it was sent is worse than
//! no language server.
//!
//! Only what the Language Server Protocol needs: objects, arrays, strings,
//! numbers, booleans, null. No streaming, no schema, no derive.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// JSON has one number type. LSP only sends integers where it matters
    /// (positions, ids), so this keeps f64 and converts on demand.
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    /// Ordered so serialization is deterministic — a wire format that changes
    /// shape between runs is a debugging tax.
    Obj(BTreeMap<String, Value>),
}

impl Value {
    /// Field lookup that never panics: a missing field and a wrong type are the
    /// same "not there" to a caller reading an untrusted message.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(m) => m.get(key),
            _ => None,
        }
    }

    /// Walk a path of field names: `v.path(&["params", "textDocument", "uri"])`.
    pub fn path(&self, keys: &[&str]) -> Option<&Value> {
        let mut cur = self;
        for k in keys {
            cur = cur.get(k)?;
        }
        Some(cur)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn obj(pairs: Vec<(&str, Value)>) -> Value {
        Value::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    pub fn num(n: impl Into<f64>) -> Value {
        Value::Num(n.into())
    }

    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_into(&mut out);
        out
    }

    fn write_into(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Num(n) => {
                // Integers must not serialize as `3.0`: LSP ids and positions are
                // integers, and some clients are strict about it.
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 9e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            Value::Str(s) => out.push_str(&crate::diag::json_string(s)),
            Value::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write_into(out);
                }
                out.push(']');
            }
            Value::Obj(map) => {
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&crate::diag::json_string(k));
                    out.push(':');
                    v.write_into(out);
                }
                out.push('}');
            }
        }
    }
}

pub fn parse(src: &str) -> Result<Value, String> {
    let bytes: Vec<char> = src.chars().collect();
    let mut p = P { c: bytes, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.c.len() {
        return Err(format!("trailing input at character {}", p.i));
    }
    Ok(v)
}

struct P {
    c: Vec<char>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(' ') | Some('\t') | Some('\n') | Some('\r')) {
            self.i += 1;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected `{}` at character {}", c, self.i))
        }
    }

    fn lit(&mut self, word: &str) -> Result<(), String> {
        for c in word.chars() {
            self.expect(c)?;
        }
        Ok(())
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => Ok(Value::Str(self.string()?)),
            Some('t') => {
                self.lit("true")?;
                Ok(Value::Bool(true))
            }
            Some('f') => {
                self.lit("false")?;
                Ok(Value::Bool(false))
            }
            Some('n') => {
                self.lit("null")?;
                Ok(Value::Null)
            }
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected `{}` at character {}", c, self.i)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect('{')?;
        let mut map = BTreeMap::new();
        self.ws();
        if self.peek() == Some('}') {
            self.i += 1;
            return Ok(Value::Obj(map));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            self.expect(':')?;
            self.ws();
            let value = self.value()?;
            map.insert(key, value);
            self.ws();
            match self.peek() {
                Some(',') => self.i += 1,
                Some('}') => {
                    self.i += 1;
                    return Ok(Value::Obj(map));
                }
                _ => return Err(format!("expected `,` or `}}` at character {}", self.i)),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(']') {
            self.i += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(',') => self.i += 1,
                Some(']') => {
                    self.i += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(format!("expected `,` or `]` at character {}", self.i)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.i += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let esc = self.peek().ok_or("unterminated escape")?;
                    self.i += 1;
                    match esc {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => out.push(self.unicode_escape()?),
                        other => {
                            return Err(format!("unknown escape `\\{}` at character {}", other, self.i))
                        }
                    }
                }
                c => out.push(c),
            }
        }
    }

    /// `\uXXXX`, including the surrogate pair encoding of astral characters —
    /// which is how an emoji in a document arrives over the wire.
    fn unicode_escape(&mut self) -> Result<char, String> {
        let hi = self.hex4()?;
        if (0xD800..0xDC00).contains(&hi) {
            // High surrogate: a low surrogate must follow.
            self.expect('\\')?;
            self.expect('u')?;
            let lo = self.hex4()?;
            if !(0xDC00..0xE000).contains(&lo) {
                return Err("high surrogate not followed by a low surrogate".to_string());
            }
            let combined = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
            return char::from_u32(combined).ok_or_else(|| "invalid surrogate pair".to_string());
        }
        char::from_u32(hi).ok_or_else(|| format!("invalid \\u escape {:04x}", hi))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut n = 0u32;
        for _ in 0..4 {
            let c = self.peek().ok_or("truncated \\u escape")?;
            let d = c.to_digit(16).ok_or_else(|| format!("`{}` is not a hex digit", c))?;
            n = n * 16 + d;
            self.i += 1;
        }
        Ok(n)
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.peek() == Some('-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.peek() == Some('.') {
            self.i += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            self.i += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let text: String = self.c[start..self.i].iter().collect();
        text.parse::<f64>().map(Value::Num).map_err(|e| format!("bad number `{}`: {}", text, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shape_lsp_actually_sends() {
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":null}}"#;
        let v = parse(msg).unwrap();
        assert_eq!(v.get("method").unwrap().as_str(), Some("initialize"));
        assert_eq!(v.get("id"), Some(&Value::num(1)));
        // `params` is the LSP wire key, not a name this project chose, so it is spelled the way
        // the protocol spells it. The naming sweep in v0.0.123 renamed this line by accident: the
        // raw string above contains quotes, which desynchronised a scan that was skipping string
        // literals by counting them. Same file and same cause as the `'"'` mistake in v0.0.9x.
        assert_eq!(v.path(&["params", "rootUri"]), Some(&Value::Null));
    }

    /// The reason this module exists: a document containing quotes and
    /// backslashes must come through byte-for-byte. Burxt source has both.
    #[test]
    fn round_trips_source_code_with_quotes_and_escapes() {
        let source = "let s: String = \"a \\\"quoted\\\" word\\n\";\nprint(s);\n";
        let encoded = Value::obj(vec![("text", Value::str(source))]).write();
        let decoded = parse(&encoded).unwrap();
        assert_eq!(decoded.get("text").unwrap().as_str(), Some(source));
    }

    #[test]
    fn decodes_unicode_and_surrogate_pairs() {
        let v = parse(r#"{"a":"caf\u00e9","b":"\ud83d\ude00"}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_str(), Some("café"));
        assert_eq!(v.get("b").unwrap().as_str(), Some("😀"));
    }

    #[test]
    fn integers_do_not_serialize_as_floats() {
        // Some LSP clients reject `"id":1.0`.
        assert_eq!(Value::num(1).write(), "1");
        assert_eq!(Value::num(0).write(), "0");
        assert_eq!(Value::num(-7).write(), "-7");
        assert_eq!(Value::num(1.5).write(), "1.5");
    }

    #[test]
    fn nesting_and_arrays_survive_a_round_trip() {
        let original = r#"{"contentChanges":[{"text":"a"},{"text":"b"}],"n":[1,2,3],"ok":true}"#;
        let v = parse(original).unwrap();
        assert_eq!(v.get("contentChanges").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(parse(&v.write()).unwrap(), v, "write then read must be identity");
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        for bad in [
            "{",
            "{\"a\"}",
            "{\"a\":}",
            "[1,]",
            "\"unterminated",
            "{\"a\":1} trailing",
            "\"\\q\"",
            "\"\\u00\"",
        ] {
            assert!(parse(bad).is_err(), "expected {:?} to be rejected", bad);
        }
    }
}
