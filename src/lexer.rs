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
    /// A string literal with no interpolation, escapes already resolved.
    Str(String),
    /// A string literal containing at least one `{expr}`. The expression is
    /// carried as source text and parsed by the parser, so the lexer settles
    /// all brace questions and leaves no ambiguity behind.
    InterpStr(Vec<StrPart>),
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
    Enum,
    Match,
    FatArrow,
    // reserved today for the OOP layers (methods, interfaces), so no program
    // written now breaks when they land
    /// `interface` stays reserved (v0.0.8) but `trait` is the chosen keyword.
    Interface,
    Is,
    SelfKw,
    Trait,
    Impl,
    For,
    Dyn,
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
    Bang,
    AmpAmp,
    PipePipe,
    // end of input
    Eof,
}

/// One piece of an interpolated string literal.
#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    /// Literal text, escapes already resolved.
    Lit(String),
    /// The source text between `{` and `}`, to be parsed as an expression.
    Expr(String),
}

impl Token {
    /// Human description for error messages — never the Rust Debug name.
    pub fn describe(&self) -> String {
        match self {
            Token::Int(n) => format!("the number {}", n),
            Token::Decimal(..) => "a decimal literal".to_string(),
            Token::Str(_) => "a string literal".to_string(),
            Token::InterpStr(_) => "an interpolated string literal".to_string(),
            Token::Ident(s) => format!("`{}`", s),
            Token::Let => "`let`".to_string(),
            Token::Mut => "`mut`".to_string(),
            Token::Print => "`print`".to_string(),
            Token::Fn => "`fn`".to_string(),
            Token::Extern => "`extern`".to_string(),
            Token::Return => "`return`".to_string(),
            Token::If => "`if`".to_string(),
            Token::Else => "`else`".to_string(),
            Token::While => "`while`".to_string(),
            Token::True => "`true`".to_string(),
            Token::False => "`false`".to_string(),
            Token::Struct => "`struct`".to_string(),
            Token::Enum => "`enum`".to_string(),
            Token::Match => "`match`".to_string(),
            Token::FatArrow => "`=>`".to_string(),
            Token::Interface => "`interface`".to_string(),
            Token::Trait => "`trait`".to_string(),
            Token::Impl => "`impl`".to_string(),
            Token::For => "`for`".to_string(),
            Token::Dyn => "`dyn`".to_string(),
            Token::Is => "`is`".to_string(),
            Token::SelfKw => "`self`".to_string(),
            Token::TyInt => "`Int`".to_string(),
            Token::TyBool => "`Bool`".to_string(),
            Token::TyString => "`String`".to_string(),
            Token::TyCInt => "`CInt`".to_string(),
            Token::TyDecimal => "`Decimal`".to_string(),
            Token::RoundHalfEven => "`RoundHalfEven`".to_string(),
            Token::RoundHalfUp => "`RoundHalfUp`".to_string(),
            Token::Colon => "`:`".to_string(),
            Token::Semicolon => "`;`".to_string(),
            Token::Comma => "`,`".to_string(),
            Token::Dot => "`.`".to_string(),
            Token::Equals => "`=`".to_string(),
            Token::Plus => "`+`".to_string(),
            Token::Minus => "`-`".to_string(),
            Token::Star => "`*`".to_string(),
            Token::Slash => "`/`".to_string(),
            Token::LParen => "`(`".to_string(),
            Token::RParen => "`)`".to_string(),
            Token::LBrace => "`{`".to_string(),
            Token::RBrace => "`}`".to_string(),
            Token::LBracket => "`[`".to_string(),
            Token::RBracket => "`]`".to_string(),
            Token::Arrow => "`->`".to_string(),
            Token::Lt => "`<`".to_string(),
            Token::Gt => "`>`".to_string(),
            Token::Le => "`<=`".to_string(),
            Token::Ge => "`>=`".to_string(),
            Token::EqEq => "`==`".to_string(),
            Token::NotEq => "`!=`".to_string(),
            Token::Bang => "`!`".to_string(),
            Token::AmpAmp => "`&&`".to_string(),
            Token::PipePipe => "`||`".to_string(),
            Token::Eof => "the end of the file".to_string(),
        }
    }
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
                if self.chars.peek() == Some(&'>') { self.chars.next(); return Ok(Token::FatArrow); }
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
            '&' => {
                self.chars.next();
                if self.chars.peek() == Some(&'&') { self.chars.next(); return Ok(Token::AmpAmp); }
                return Err(
                    "Burxt has no bitwise `&` — did you mean `&&` (logical and)?".to_string(),
                );
            }
            '|' => {
                self.chars.next();
                if self.chars.peek() == Some(&'|') { self.chars.next(); return Ok(Token::PipePipe); }
                return Err(
                    "Burxt has no bitwise `|` — did you mean `||` (logical or)?".to_string(),
                );
            }
            '!' => {
                self.chars.next();
                if self.chars.peek() == Some(&'=') { self.chars.next(); return Ok(Token::NotEq); }
                return Ok(Token::Bang);
            }
            _ => {}
        }

        // string literal
        if c == '"' {
            return self.lex_string();
        }

        // `$19.99` — a money literal. Pure sugar for a Decimal<2> value: the
        // `$` says "money, scale 2" so the digits need not spell the cents.
        if c == '$' {
            self.chars.next();
            return self.lex_money();
        }

        // number (int or decimal), possibly with a `%` or unit suffix
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
                // `8.25%` is exactly 0.0825: the same digits, two more
                // fractional places. Never 8.25/100 through a division, and
                // never a float.
                if self.chars.peek() == Some(&'%') {
                    self.chars.next();
                    return Ok(Token::Decimal(unscaled, scale + 2));
                }
                return Ok(Token::Decimal(unscaled, scale));
            }
        }

        let value: i64 = int_part
            .parse()
            .map_err(|_| format!("integer literal too large: {}", int_part))?;
        if self.chars.peek() == Some(&'%') {
            self.chars.next();
            // `50%` is exactly 0.50.
            return Ok(Token::Decimal(value, 2));
        }
        Ok(Token::Int(value))
    }

    /// Lex the digits after a `$`. The result is exactly a `Decimal<2>`
    /// literal — `$5` and `$5.00` and `$5.0` are the same value — so money
    /// literals are sugar over the existing exact-decimal type, never a new
    /// one, and every existing decimal rule applies unchanged.
    fn lex_money(&mut self) -> Result<Token, String> {
        if !matches!(self.chars.peek(), Some(c) if c.is_ascii_digit()) {
            return Err(
                "`$` must be followed by digits — a money literal looks like `$19.99`"
                    .to_string(),
            );
        }
        match self.lex_number()? {
            Token::Int(n) => {
                // `$5` means five whole units: 500 cents.
                let unscaled = n
                    .checked_mul(100)
                    .ok_or("money literal too large for Decimal<2>")?;
                Ok(Token::Decimal(unscaled, 2))
            }
            Token::Decimal(unscaled, scale) => {
                if scale > 2 {
                    return Err(format!(
                        "`$` money is Decimal<2>, but this literal has {} fractional \
                         digits. Write it with at most 2, or use an explicit \
                         Decimal<{}> without the `$`.",
                        scale, scale
                    ));
                }
                // widen 1 fractional digit to 2 ($5.5 == $5.50), exactly
                let factor = 10i64.pow(2 - scale);
                let unscaled = unscaled
                    .checked_mul(factor)
                    .ok_or("money literal too large for Decimal<2>")?;
                Ok(Token::Decimal(unscaled, 2))
            }
            other => Err(format!(
                "expected digits after `$`, found {}",
                other.describe()
            )),
        }
    }

    /// Lex a double-quoted, single-line string literal. Escapes are resolved
    /// here: exactly \\ \" \n \t. There is no \0 — by construction a Burxt
    /// string never contains an interior NUL.
    fn lex_string(&mut self) -> Result<Token, String> {
        self.chars.next(); // opening quote
        let mut s = String::new();
        let mut parts: Vec<StrPart> = Vec::new();
        loop {
            match self.chars.next() {
                Some('"') => {
                    if parts.is_empty() {
                        return Ok(Token::Str(s));
                    }
                    if !s.is_empty() {
                        parts.push(StrPart::Lit(s));
                    }
                    return Ok(Token::InterpStr(parts));
                }
                // `{expr}` interpolates. A BARE `{` used to be an ordinary
                // character, so accepting it silently would change what
                // existing programs mean — instead a literal brace must now be
                // written `\{`, and the error says so.
                Some('{') => {
                    if !s.is_empty() {
                        parts.push(StrPart::Lit(std::mem::take(&mut s)));
                    }
                    let mut expr = String::new();
                    let mut depth = 1usize;
                    loop {
                        match self.chars.next() {
                            Some('{') => {
                                depth += 1;
                                expr.push('{');
                            }
                            Some('}') => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                expr.push('}');
                            }
                            None | Some('\n') => {
                                return Err(
                                    "unterminated interpolation — close it with `}` \
                                     before the end of the line"
                                        .to_string(),
                                )
                            }
                            // The string ended before the interpolation closed.
                            // Overwhelmingly this means a literal brace was
                            // intended, so lead with that advice.
                            Some('"') => {
                                return Err(
                                    "a literal `{` in a string must be written `\\{` — \
                                     an unescaped `{` starts a `{expr}` interpolation. \
                                     (A string literal cannot appear inside `{...}` \
                                     yet either.)"
                                        .to_string(),
                                )
                            }
                            Some(c) => expr.push(c),
                        }
                    }
                    if expr.trim().is_empty() {
                        return Err(
                            "empty interpolation `{}` — put an expression inside it, \
                             or write `\\{` and `\\}` for literal braces"
                                .to_string(),
                        );
                    }
                    parts.push(StrPart::Expr(expr));
                }
                Some('}') => {
                    return Err(
                        "a literal `}` in a string must be written `\\}` — a bare `}` \
                         only closes a `{expr}` interpolation"
                            .to_string(),
                    )
                }
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
                    Some('{') => s.push('{'),
                    Some('}') => s.push('}'),
                    Some(other) if other != '\n' => {
                        return Err(format!(
                            "unknown escape `\\{}` — Burxt strings support \\\\, \\\", \
                             \\n, \\t, \\{{ and \\}}",
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
            "enum" => Token::Enum,
            "match" => Token::Match,
            "interface" => Token::Interface,
            "is" => Token::Is,
            "self" => Token::SelfKw,
            "trait" => Token::Trait,
            "impl" => Token::Impl,
            "for" => Token::For,
            "dyn" => Token::Dyn,
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
