//! Lexer: turns Burxt source text into a flat stream of tokens.
//!
//! Crucially, decimal literals like `19.99` are captured as their raw digit
//! text and split into (integer part, fractional part) — never parsed through
//! a floating-point type. Exactness starts at the very first stage.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // literals
    Int(i64),
    /// A decimal literal captured exactly: (unscaled value, scale).
    /// `19.99` -> Decimal(1999, 2). `0.5` -> Decimal(5, 1).
    Decimal(i64, u32),
    /// A string literal, escapes already resolved.
    Str(String),
    // identifiers & keywords
    Ident(String),
    Let,
    Mut,
    Print,
    Fn,
    Extern,
    Return,
    If,
    Else,
    While,
    True,
    False,
    Struct,
    // reserved today for the OOP layers (methods, interfaces), so no program
    // written now breaks when they land
    Interface,
    Is,
    SelfKw,
    // type keywords
    TyInt,
    TyBool,
    TyString,
    TyDecimal,
    /// C's 32-bit int — only meaningful in extern fn signatures.
    TyCInt,
    RoundHalfEven,
    RoundHalfUp,
    // punctuation
    Colon,
    Semicolon,
    Comma,
    Dot,
    Equals,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Arrow,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    NotEq,
    // end of input
    Eof,
}

pub struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer {
            chars: src.chars().peekable(),
        }
    }

    /// Tokenize the whole input. Returns an error string on the first bad char.
    pub fn tokenize(mut self) -> Result<Vec<Token>, String> {
        let mut out = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok == Token::Eof;
            out.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(out)
    }

    fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace_and_comments();

        let c = match self.chars.peek() {
            None => return Ok(Token::Eof),
            Some(&c) => c,
        };

        // punctuation (two-character operators first: they extend a one-char one)
        match c {
            ':' => { self.chars.next(); return Ok(Token::Colon); }
            ';' => { self.chars.next(); return Ok(Token::Semicolon); }
            ',' => { self.chars.next(); return Ok(Token::Comma); }
            // a '.' between digits was already consumed by lex_number;
            // a solitary '.' is field access
            '.' => { self.chars.next(); return Ok(Token::Dot); }
            '=' => {
                self.chars.next();
                if self.chars.peek() == Some(&'=') { self.chars.next(); return Ok(Token::EqEq); }
                return Ok(Token::Equals);
            }
            '+' => { self.chars.next(); return Ok(Token::Plus); }
            '-' => {
                self.chars.next();
                if self.chars.peek() == Some(&'>') { self.chars.next(); return Ok(Token::Arrow); }
                return Ok(Token::Minus);
            }
            '*' => { self.chars.next(); return Ok(Token::Star); }
            // a solitary '/' is division; '//' was already consumed as a comment
            '/' => { self.chars.next(); return Ok(Token::Slash); }
            '(' => { self.chars.next(); return Ok(Token::LParen); }
            ')' => { self.chars.next(); return Ok(Token::RParen); }
            '{' => { self.chars.next(); return Ok(Token::LBrace); }
            '}' => { self.chars.next(); return Ok(Token::RBrace); }
            '[' => { self.chars.next(); return Ok(Token::LBracket); }
            ']' => { self.chars.next(); return Ok(Token::RBracket); }
            '<' => {
                self.chars.next();
                if self.chars.peek() == Some(&'=') { self.chars.next(); return Ok(Token::Le); }
                return Ok(Token::Lt);
            }
            '>' => {
                self.chars.next();
                if self.chars.peek() == Some(&'=') { self.chars.next(); return Ok(Token::Ge); }
                return Ok(Token::Gt);
            }
            '!' => {
                self.chars.next();
                if self.chars.peek() == Some(&'=') { self.chars.next(); return Ok(Token::NotEq); }
                return Err("unexpected character: '!' (did you mean '!='?)".to_string());
            }
            _ => {}
        }

        // string literal
        if c == '"' {
            return self.lex_string();
        }

        // number (int or decimal)
        if c.is_ascii_digit() {
            return self.lex_number();
        }

        // identifier / keyword
        if c.is_ascii_alphabetic() || c == '_' {
            return Ok(self.lex_ident_or_keyword());
        }

        // '#' is claimed for compile-time attributes before anything else can
        // use it — the future surface of verification contracts.
        if c == '#' {
            return Err(
                "'#' is reserved for attributes — #[invariant(...)], #[ensures(...)] — \
                 coming with refinement types"
                    .to_string(),
            );
        }

        Err(format!("unexpected character: {:?}", c))
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.chars.peek() {
                Some(&c) if c.is_whitespace() => { self.chars.next(); }
                // line comment: // ... to end of line
                Some(&'/') => {
                    // need to look ahead for a second '/'
                    let mut clone = self.chars.clone();
                    clone.next();
                    if clone.peek() == Some(&'/') {
                        // consume until newline
                        while let Some(&c) = self.chars.peek() {
                            self.chars.next();
                            if c == '\n' { break; }
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    /// Lex an integer or a decimal. Decimals are captured EXACTLY: we count the
    /// fractional digits to derive the scale and build the unscaled integer.
    fn lex_number(&mut self) -> Result<Token, String> {
        let mut int_part = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() {
                int_part.push(c);
                self.chars.next();
            } else {
                break;
            }
        }

        // Is there a fractional part?
        if self.chars.peek() == Some(&'.') {
            // Look ahead: only treat '.' as decimal point if a digit follows.
            let mut clone = self.chars.clone();
            clone.next();
            if matches!(clone.peek(), Some(c) if c.is_ascii_digit()) {
                self.chars.next(); // consume '.'
                let mut frac_part = String::new();
                while let Some(&c) = self.chars.peek() {
                    if c.is_ascii_digit() {
                        frac_part.push(c);
                        self.chars.next();
                    } else {
                        break;
                    }
                }
                let scale = frac_part.len() as u32;
                let combined = format!("{}{}", int_part, frac_part);
                let unscaled: i64 = combined
                    .parse()
                    .map_err(|_| format!("decimal literal too large: {}.{}", int_part, frac_part))?;
                return Ok(Token::Decimal(unscaled, scale));
            }
        }

        let value: i64 = int_part
            .parse()
            .map_err(|_| format!("integer literal too large: {}", int_part))?;
        Ok(Token::Int(value))
    }

    /// Lex a double-quoted, single-line string literal. Escapes are resolved
    /// here: exactly \\ \" \n \t. There is no \0 — by construction a Burxt
    /// string never contains an interior NUL.
    fn lex_string(&mut self) -> Result<Token, String> {
        self.chars.next(); // opening quote
        let mut s = String::new();
        loop {
            match self.chars.next() {
                Some('"') => return Ok(Token::Str(s)),
                None | Some('\n') => {
                    return Err(
                        "unterminated string literal — close it with `\"` before \
                         the end of the line"
                            .to_string(),
                    )
                }
                Some('\\') => match self.chars.next() {
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(other) if other != '\n' => {
                        return Err(format!(
                            "unknown escape `\\{}` — Burxt strings support \\\\, \\\", \\n and \\t",
                            other
                        ))
                    }
                    // backslash at end of line / end of input: the string
                    // never closed — say that, not "unknown escape".
                    _ => {
                        return Err(
                            "unterminated string literal — close it with `\"` before \
                             the end of the line"
                                .to_string(),
                        )
                    }
                },
                // A raw NUL would silently truncate the NUL-terminated bytes
                // at runtime — silent data loss, so it cannot appear at all.
                Some('\0') => {
                    return Err(
                        "a raw NUL byte cannot appear in a string literal — Burxt \
                         strings are NUL-terminated"
                            .to_string(),
                    )
                }
                Some(c) => s.push(c),
            }
        }
    }

    fn lex_ident_or_keyword(&mut self) -> Token {
        let mut s = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        match s.as_str() {
            "let" => Token::Let,
            "mut" => Token::Mut,
            "print" => Token::Print,
            "while" => Token::While,
            "fn" => Token::Fn,
            "extern" => Token::Extern,
            "return" => Token::Return,
            "if" => Token::If,
            "else" => Token::Else,
            "true" => Token::True,
            "false" => Token::False,
            "struct" => Token::Struct,
            "interface" => Token::Interface,
            "is" => Token::Is,
            "self" => Token::SelfKw,
            "Int" => Token::TyInt,
            "Bool" => Token::TyBool,
            "String" => Token::TyString,
            "CInt" => Token::TyCInt,
            "Decimal" => Token::TyDecimal,
            "RoundHalfEven" => Token::RoundHalfEven,
            "RoundHalfUp" => Token::RoundHalfUp,
            _ => Token::Ident(s),
        }
    }
}
