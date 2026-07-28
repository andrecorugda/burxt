//! Lexer: turns Burxt source text into a flat stream of tokens.
//!
//! Crucially, decimal literals like `19.99` are captured as their raw digit
//! text and split into (integer part, fractional part) — never parsed through
//! a floating-point type. Exactness starts at the very first stage.
//!
//! Every token carries a `Span` — the byte range it came from — because an error
//! that cannot say WHERE is an error an editor cannot show.

use crate::diag::{Diagnostic, Span};

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
    /// `as` — introduces a boundary marshaller in an `extern fn` signature.
    As,
    /// `return tail f(x)` — a tail call the compiler must guarantee.
    Tail,
    /// `pure fn` — the result depends only on the arguments, and the compiler
    /// checks it: no I/O, no FFI, no impure calls.
    Pure,
    /// `break` — leave the enclosing loop.
    Break,
    /// `continue` — go straight to the enclosing loop's next test.
    Continue,
    If,
    Else,
    While,
    True,
    False,
    Struct,
    Enum,
    Region,
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
    In,
    Dyn,
    // type keywords
    TyInt,
    TyBool,
    TyString,
    TyDecimal,
    /// C's 32-bit int — only meaningful in extern fn signatures.
    TyCInt,
    /// C's `double` — an FFI-only type, so a lossy crossing can be NAMED and
    /// therefore refused. Burxt itself still has no float type.
    TyCDouble,
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
    /// `+=`, `-=`, `*=`: sugar the parser expands into `x = x <op> value`.
    PlusEq,
    MinusEq,
    StarEq,
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
    /// `?` — the failure shortcut. See spec/M8-ERRORS.md §1a.
    Question,
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
            Token::Mut => "`mutable`".to_string(),
            Token::Print => "`print`".to_string(),
            Token::Fn => "`function`".to_string(),
            Token::Extern => "`external`".to_string(),
            Token::Return => "`return`".to_string(),
            Token::As => "`as`".to_string(),
            Token::Tail => "`tail`".to_string(),
            Token::Pure => "`pure`".to_string(),
            Token::Break => "`break`".to_string(),
            Token::Continue => "`continue`".to_string(),
            Token::If => "`if`".to_string(),
            Token::Else => "`else`".to_string(),
            Token::While => "`while`".to_string(),
            Token::True => "`true`".to_string(),
            Token::False => "`false`".to_string(),
            Token::Struct => "`record`".to_string(),
            Token::Region => "`region`".to_string(),
            Token::Enum => "`enum`".to_string(),
            Token::Match => "`match`".to_string(),
            Token::FatArrow => "`=>`".to_string(),
            Token::Interface => "`interface`".to_string(),
            Token::Trait => "`trait`".to_string(),
            Token::Impl => "`implement`".to_string(),
            Token::For => "`for`".to_string(),
            Token::In => "`in`".to_string(),
            Token::Dyn => "`dynamic`".to_string(),
            Token::Is => "`is`".to_string(),
            Token::SelfKw => "`self`".to_string(),
            Token::TyInt => "`Int`".to_string(),
            Token::TyBool => "`Bool`".to_string(),
            Token::TyString => "`String`".to_string(),
            Token::TyCInt => "`CInt`".to_string(),
            Token::TyCDouble => "`CDouble`".to_string(),
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
            Token::PlusEq => "`+=`".to_string(),
            Token::MinusEq => "`-=`".to_string(),
            Token::StarEq => "`*=`".to_string(),
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
            Token::Question => "`?`".to_string(),
            Token::AmpAmp => "`&&`".to_string(),
            Token::PipePipe => "`||`".to_string(),
            Token::Eof => "the end of the file".to_string(),
        }
    }
}

pub struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    /// Byte offset of the next character. Tracked here rather than by switching
    /// to `char_indices` so every scanning site stays as it reads, and so the
    /// offset is maintained in exactly one place: `bump`.
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer {
            chars: src.chars().peekable(),
            pos: 0,
        }
    }

    /// Consume one character, keeping the byte offset honest for multi-byte
    /// characters — a span measured in bytes must be measured in bytes.
    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next();
        if let Some(c) = c {
            self.pos += c.len_utf8();
        }
        c
    }

    fn peek_char(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    /// Tokenize the whole input, pairing every token with the source it came
    /// from. The span is what lets an editor underline the right characters.
    pub fn tokenize(mut self) -> Result<Vec<(Token, Span)>, Diagnostic> {
        let mut out = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let start = self.pos;
            let tok = match self.next_token() {
                Ok(t) => t,
                // A lexer error is always inside the token being scanned, so
                // the span runs from that token's first character to here.
                Err(message) => {
                    let end = self.pos.max(start + 1);
                    return Err(Diagnostic::new(message, Span::new(start, end)));
                }
            };
            let is_eof = tok == Token::Eof;
            out.push((tok, Span::new(start, self.pos)));
            if is_eof {
                break;
            }
        }
        Ok(out)
    }

    fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace_and_comments();

        let c = match self.peek_char() {
            None => return Ok(Token::Eof),
            Some(&c) => c,
        };

        // punctuation (two-character operators first: they extend a one-char one)
        match c {
            ':' => { self.bump(); return Ok(Token::Colon); }
            ';' => { self.bump(); return Ok(Token::Semicolon); }
            ',' => { self.bump(); return Ok(Token::Comma); }
            // a '.' between digits was already consumed by lex_number;
            // a solitary '.' is field access
            '.' => { self.bump(); return Ok(Token::Dot); }
            '=' => {
                self.bump();
                if self.peek_char() == Some(&'=') { self.bump(); return Ok(Token::EqEq); }
                if self.peek_char() == Some(&'>') { self.bump(); return Ok(Token::FatArrow); }
                return Ok(Token::Equals);
            }
            '+' => {
                self.bump();
                // `x += 1` is sugar the PARSER expands into `x = x + 1`, so one token here
                // and no new statement kind, no typecheck rule, no lowering. There is
                // deliberately no `x++`: an expression with a side effect is the class of
                // thing this language refuses, and `+=` as a statement is the brevity
                // without the trap.
                if self.peek_char() == Some(&'=') { self.bump(); return Ok(Token::PlusEq); }
                return Ok(Token::Plus);
            }
            '-' => {
                self.bump();
                if self.peek_char() == Some(&'=') { self.bump(); return Ok(Token::MinusEq); }
                if self.peek_char() == Some(&'>') { self.bump(); return Ok(Token::Arrow); }
                return Ok(Token::Minus);
            }
            '*' => {
                self.bump();
                if self.peek_char() == Some(&'=') { self.bump(); return Ok(Token::StarEq); }
                return Ok(Token::Star);
            }
            // a solitary '/' is division; '//' was already consumed as a comment
            '/' => {
                self.bump();
                // `/* ... */` is the one thing a reader might reasonably try and not find.
                // Burxt has line comments only — one way to write a comment, and no rule
                // to learn about nesting — so say that instead of "expected statement,
                // found `/`", which is what the parser used to report two tokens later.
                if self.peek_char() == Some(&'*') {
                    return Err(
                        "Burxt has line comments only: `// like this`. There is no \
                         `/* ... */`, so there is no nesting rule to get wrong — comment \
                         out a block by putting `//` on each line, which every editor \
                         will do for you."
                            .to_string(),
                    );
                }
                return Ok(Token::Slash);
            }
            '(' => { self.bump(); return Ok(Token::LParen); }
            ')' => { self.bump(); return Ok(Token::RParen); }
            '{' => { self.bump(); return Ok(Token::LBrace); }
            '}' => { self.bump(); return Ok(Token::RBrace); }
            '[' => { self.bump(); return Ok(Token::LBracket); }
            ']' => { self.bump(); return Ok(Token::RBracket); }
            '<' => {
                self.bump();
                if self.peek_char() == Some(&'=') { self.bump(); return Ok(Token::Le); }
                return Ok(Token::Lt);
            }
            '>' => {
                self.bump();
                if self.peek_char() == Some(&'=') { self.bump(); return Ok(Token::Ge); }
                return Ok(Token::Gt);
            }
            '&' => {
                self.bump();
                if self.peek_char() == Some(&'&') { self.bump(); return Ok(Token::AmpAmp); }
                return Err(
                    "Burxt has no bitwise `&` — did you mean `&&` (logical and)?".to_string(),
                );
            }
            '|' => {
                self.bump();
                if self.peek_char() == Some(&'|') { self.bump(); return Ok(Token::PipePipe); }
                return Err(
                    "Burxt has no bitwise `|` — did you mean `||` (logical or)?".to_string(),
                );
            }
            '!' => {
                self.bump();
                if self.peek_char() == Some(&'=') { self.bump(); return Ok(Token::NotEq); }
                return Ok(Token::Bang);
            }
            '?' => { self.bump(); return Ok(Token::Question); }
            _ => {}
        }

        // string literal
        if c == '"' {
            return self.lex_string();
        }

        // `$19.99` — a money literal. Pure sugar for a Decimal<2> value: the
        // `$` says "money, scale 2" so the digits need not spell the cents.
        if c == '$' {
            self.bump();
            return self.lex_money();
        }

        // number (int or decimal), possibly with a `%` or unit suffix
        if c.is_ascii_digit() {
            return self.lex_number();
        }

        // identifier / keyword
        if c.is_ascii_alphabetic() || c == '_' {
            return self.lex_ident_or_keyword();
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
            match self.peek_char() {
                Some(&c) if c.is_whitespace() => { self.bump(); }
                // line comment: // ... to end of line
                Some(&'/') => {
                    // need to look ahead for a second '/'
                    let mut clone = self.chars.clone();
                    clone.next();
                    if clone.peek() == Some(&'/') {
                        // consume until newline
                        while let Some(&c) = self.peek_char() {
                            self.bump();
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
        while let Some(&c) = self.peek_char() {
            if c.is_ascii_digit() {
                int_part.push(c);
                self.bump();
            } else {
                break;
            }
        }

        // Is there a fractional part?
        if self.peek_char() == Some(&'.') {
            // Look ahead: only treat '.' as decimal point if a digit follows.
            let mut clone = self.chars.clone();
            clone.next();
            if matches!(clone.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump(); // consume '.'
                let mut frac_part = String::new();
                while let Some(&c) = self.peek_char() {
                    if c.is_ascii_digit() {
                        frac_part.push(c);
                        self.bump();
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
                if self.peek_char() == Some(&'%') {
                    self.bump();
                    return Ok(Token::Decimal(unscaled, scale + 2));
                }
                return Ok(Token::Decimal(unscaled, scale));
            }
        }

        let value: i64 = int_part
            .parse()
            .map_err(|_| format!("integer literal too large: {}", int_part))?;
        if self.peek_char() == Some(&'%') {
            self.bump();
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
        if !matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
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
        self.bump(); // opening quote
        let mut s = String::new();
        let mut parts: Vec<StrPart> = Vec::new();
        loop {
            match self.bump() {
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
                        match self.bump() {
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
                Some('\\') => match self.bump() {
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

    fn lex_ident_or_keyword(&mut self) -> Result<Token, String> {
        let mut s = String::new();
        while let Some(&c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        // Every keyword in this language is the word it means. `fn`, `mut`, `impl`, `dyn`,
        // `extern` and `struct` were inherited clippings from Rust, and they sat badly beside
        // `allocates`, `requires` and `decreases` — twenty-five words spelled out against
        // five abbreviated. Renamed in v0.0.98, with the old spellings kept reserved for one
        // job only: naming the new one. A clean break with a signpost, rather than two ways
        // to write one thing.
        //
        // Refused BEFORE the table, so the table below stays a plain list of
        // `"word" => Token::Variant` — which is the form the editor-grammar test reads.
        if let Some(message) = renamed_keyword(&s) {
            return Err(message);
        }
        Ok(match s.as_str() {
            "let" => Token::Let,
            "mutable" => Token::Mut,
            "print" => Token::Print,
            "while" => Token::While,
            "function" => Token::Fn,
            "external" => Token::Extern,
            "return" => Token::Return,
            "as" => Token::As,
            "tail" => Token::Tail,
            // `allocates`, `requires`, `ensures` and `decreases` are deliberately
            // ABSENT from this table: they are contextual, recognised by the parser
            // where they are the only thing that can appear, and ordinary
            // identifiers everywhere else. `let allocates: Int = 0;` is legal.
            //
            // Reserving a word globally to recognise it in one position is
            // over-collection, and the list only grows: every guarantee this language
            // adds is a declared word. `scaled` in `as scaled` was contextual from
            // the start; these follow it.
            "pure" => Token::Pure,
            "break" => Token::Break,
            "continue" => Token::Continue,
            "if" => Token::If,
            "else" => Token::Else,
            "true" => Token::True,
            "false" => Token::False,
            "record" => Token::Struct,
            "region" => Token::Region,
            "enum" => Token::Enum,
            "match" => Token::Match,
            "interface" => Token::Interface,
            "is" => Token::Is,
            "self" => Token::SelfKw,
            "trait" => Token::Trait,
            "implement" => Token::Impl,
            "for" => Token::For,
            // `for x in xs { }`. `for` was already reserved by `impl Trait for Type`;
            // `in` joins it rather than becoming contextual, because `for` opens a
            // statement and so may an identifier — telling `for x in xs` from
            // `format(x);` would take three tokens of lookahead. Every reader already
            // expects both reserved, which is the test that matters.
            "in" => Token::In,
            "dynamic" => Token::Dyn,
            "Int" => Token::TyInt,
            "Bool" => Token::TyBool,
            "String" => Token::TyString,
            "CInt" => Token::TyCInt,
            "CDouble" => Token::TyCDouble,
            "Decimal" => Token::TyDecimal,
            "RoundHalfEven" => Token::RoundHalfEven,
            "RoundHalfUp" => Token::RoundHalfUp,
            _ => Token::Ident(s),
        })
    }
}

/// The six words Burxt used to spell short. Answers the message an old spelling gets — which
/// names the new word AND why that word, because a rename a reader cannot see the reason for
/// is a rename they will resent.
fn renamed_keyword(word: &str) -> Option<String> {
    let (new, why) = match word {
        "fn" => ("function", "a function is a function"),
        "mut" => ("mutable", "a binding that can change is mutable"),
        "impl" => ("implement", "`implement Priced for Book` reads as the sentence it is"),
        "dyn" => ("dynamic", "the decision it names is made dynamically, at run time"),
        "extern" => ("external", "the function it names is external to this program"),
        "struct" => (
            "record",
            "named fields, copied by value, with no inheritance and no hidden header — \
             which is what a record is, and what a class is not",
        ),
        _ => return None,
    };
    Some(format!(
        "Burxt spells this `{}`, not `{}`: {}. Every keyword in this language is the word it \
         means — which is why `allocates` and `decreases` are not `alloc` and `dec`.",
        new, word, why
    ))
}
