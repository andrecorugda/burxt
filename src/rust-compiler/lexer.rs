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
    /// `const NAME: Type = <compile-time value>;` — a name for a literal.
    ///
    /// A separate token from `Let` rather than a flag on it, because the two are not
    /// the same construct wearing different hats: a `const` is an ITEM (it may only
    /// appear at the top level, and it is in scope inside every function), while a
    /// `let` is a STATEMENT (it is in scope from its own line to the end of its
    /// block, and a function body cannot see one). See `ast::ConstDef`.
    Const,
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
    /// C2. `public function`, `public class` — reachable from a package that depends on this one.
    Public,
    /// `break` — leave the enclosing loop.
    Break,
    /// `continue` — go straight to the enclosing loop's next test.
    Continue,
    If,
    Else,
    While,
    True,
    False,
    Class,
    Private,
    Enum,
    Region,
    Match,
    FatArrow,
    // reserved today for the OOP layers (methods, interfaces), so no program
    // written now breaks when they land
    /// `interface` stays reserved (v0.0.8) but `interface` is the chosen keyword.
    Interface,
    Is,
    SelfKw,

    Impl,

    Implements,
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
    /// A sized C integer — `i32` `u8` `u32` `u64`, roadmap A7. ONE token carrying the two numbers
    /// that distinguish them, for the same reason `Type::Width` is one variant: the parser has
    /// nothing to decide, so four tokens would be four identical arms.
    TyWidth { bits: u32, signed: bool },
    TyCPointer,
    PrintError,
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
    /// `..` — the range in `for i in 0..n`, and nothing else. See `StmtKind::ForRange`
    /// for why a range is a `for` construct rather than a value.
    ///
    /// A real token rather than two `Dot`s recognised by the parser, for three reasons:
    /// a two-`Dot` reading would also accept `0 . . 3` and `x . . y`; a diagnostic can
    /// then say `` `..` `` instead of `` `.` `` twice; and the editor support, the
    /// language server and the site reference all enumerate the token set, so a lexeme
    /// the lexer does not name is a lexeme they cannot highlight.
    DotDot,
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
    /// `?` — the failure shortcut. See spec/1.0/M8-ERRORS.md §1a.
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
            Token::Const => "`const`".to_string(),
            Token::Print => "`print`".to_string(),
            Token::Fn => "`function`".to_string(),
            Token::Extern => "`external`".to_string(),
            Token::Return => "`return`".to_string(),
            Token::As => "`as`".to_string(),
            Token::Tail => "`tail`".to_string(),
            Token::Pure => "`pure`".to_string(),
            Token::Public => "`public`".to_string(),
            Token::Break => "`break`".to_string(),
            Token::Continue => "`continue`".to_string(),
            Token::If => "`if`".to_string(),
            Token::Else => "`else`".to_string(),
            Token::While => "`while`".to_string(),
            Token::True => "`true`".to_string(),
            Token::False => "`false`".to_string(),
            Token::Class => "`class`".to_string(),
            Token::Private => "`private`".to_string(),
            Token::Region => "`region`".to_string(),
            Token::Enum => "`enum`".to_string(),
            Token::Match => "`match`".to_string(),
            Token::FatArrow => "`=>`".to_string(),
            Token::Interface => "`interface`".to_string(),
            Token::Impl => "`implement`".to_string(),
            Token::Implements => "`implements`".to_string(),
            Token::For => "`for`".to_string(),
            Token::In => "`in`".to_string(),
            Token::Dyn => "`dynamic`".to_string(),
            Token::Is => "`is`".to_string(),
            Token::SelfKw => "`self`".to_string(),
            Token::TyInt => "`Int`".to_string(),
            Token::TyBool => "`Bool`".to_string(),
            Token::TyString => "`String`".to_string(),
            Token::TyCInt => "`CInt`".to_string(),
            Token::TyWidth { bits, signed } => {
                format!("`{}{}`", if *signed { "i" } else { "u" }, bits)
            }
            Token::TyCPointer => "`CPointer`".to_string(),
            Token::PrintError => "`print_error`".to_string(),
            Token::TyCDouble => "`CDouble`".to_string(),
            Token::TyDecimal => "`Decimal`".to_string(),
            Token::RoundHalfEven => "`RoundHalfEven`".to_string(),
            Token::RoundHalfUp => "`RoundHalfUp`".to_string(),
            Token::Colon => "`:`".to_string(),
            Token::Semicolon => "`;`".to_string(),
            Token::Comma => "`,`".to_string(),
            Token::Dot => "`.`".to_string(),
            Token::DotDot => "`..`".to_string(),
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
                    // **`start + 1` was wrong, and it crashed the compiler.** An unknown
                    // character errors WITHOUT bumping, so `self.pos == start` and the span
                    // ended one BYTE in — which for `é` is the middle of a two-byte
                    // character. `diag.rs` then sliced the source at that offset and
                    // panicked: `let é: Int = ;` produced a Rust backtrace and exit 101
                    // instead of a diagnostic. A span is measured in bytes, but it must
                    // still land on character boundaries, because the thing that renders it
                    // counts characters.
                    //
                    // Two cases, and `max` cannot serve both: if the scan advanced, `pos` is
                    // already a boundary and is the right end. If it did not, the end is the
                    // whole width of the character that could not be read.
                    let end = if self.pos > start {
                        self.pos
                    } else {
                        start + self.chars.peek().map(|c| c.len_utf8()).unwrap_or(1)
                    };
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
            // a solitary '.' is field access, and `..` is a range.
            //
            // THE ONE REAL AMBIGUITY IN A6, and it is settled entirely by the lookahead
            // `lex_number` already had. `0..3` and `0.5` begin with the same two bytes.
            // What resolves them is that `lex_number` claims a `.` only when a DIGIT
            // follows it (see the "Is there a fractional part?" block below); in `0..3`
            // the byte after the first `.` is another `.`, so the number ends at `0` and
            // control reaches here with the two dots unconsumed. Nothing about decimal
            // lexing had to change — the lookahead that already made `x.0` a field access
            // is the same lookahead that makes `0..3` a range. Measured, not reasoned:
            // `tests/fail/range_start_must_be_an_int.bx` contains `1.0..2.0`, which only
            // lexes into three tokens if this is true.
            //
            // The cases that had to keep working, each with a fixture:
            //   `0.5` and `$1.50` — claimed by lex_number, never reach here
            //   `x.field`         — '.' then a letter, so one Dot
            //   `1..2`            — Int, DotDot, Int
            //   `1.0..2.0`        — Decimal, DotDot, Decimal; refused by the CHECKER,
            //                       because a range counts and a Decimal is not a count
            '.' => {
                self.bump();
                if self.peek_char() == Some(&'.') {
                    self.bump();
                    // `..=` and `...` are refused HERE, where they are read, rather than
                    // left to fall apart in the parser as `..` followed by `=` or `.`.
                    // Both are the spelling of an inclusive range in some other language,
                    // so both arrive as an honest guess and deserve an answer instead of
                    // "expected an expression, found `=`". Burxt has ONE range form: see
                    // `StmtKind::ForRange` for why a second one differing by a single
                    // character is a reading hazard in a language whose claim is that a
                    // reviewer sees the bug.
                    if matches!(self.peek_char(), Some('=') | Some('.')) {
                        return Err(
                            "a Burxt range is exclusive and spelled `..` — there is no \
                             `..=` or `...`. `0..3` is 0, 1, 2; for 0 through 3 write \
                             `0..4`."
                                .to_string(),
                        );
                    }
                    return Ok(Token::DotDot);
                }
                return Ok(Token::Dot);
            }
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
                    "Burxt has no bitwise `&` — write `bit_and(a, b)`, or did you mean `&&` \
                     (logical and)? The bit operations are NAMED because `a & b == c` means \
                     `a & (b == c)` in C, and a precedence table a reviewer has to remember is \
                     the opposite of what this language is for."
                        .to_string(),
                );
            }
            '|' => {
                self.bump();
                if self.peek_char() == Some(&'|') { self.bump(); return Ok(Token::PipePipe); }
                return Err(
                    "Burxt has no bitwise `|` — write `bit_or(a, b)`, or did you mean `||` \
                     (logical or)?"
                        .to_string(),
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
        // `0x...` — hexadecimal, for bit work only.
        //
        // Added with the bit operations (v0.0.199) and for their sake: a mask, a CRC polynomial or a
        // protocol field is written in hex everywhere it is SPECIFIED, and a reviewer checking
        // `0xEDB88320` against the standard that defines it should be comparing the same characters.
        // Writing it as 3988292384 is the same number and a worse review.
        //
        // No hex Decimal, deliberately: a scale is a decimal-digit count, so `0x1.8` would have to
        // mean something about base ten that base sixteen cannot say.
        if self.peek_char() == Some(&'0') {
            let mut clone = self.chars.clone();
            clone.next();
            if matches!(clone.peek(), Some(&'x') | Some(&'X')) {
                self.bump();
                self.bump();
                let mut digits = String::new();
                while let Some(&c) = self.peek_char() {
                    if c.is_ascii_hexdigit() {
                        digits.push(c);
                        self.bump();
                    } else if c == '_' {
                        self.bump();          // `0xFFFF_FFFF` reads better and means the same
                    } else {
                        break;
                    }
                }
                if digits.is_empty() {
                    return Err(
                        "`0x` needs hexadecimal digits after it, e.g. `0xFF`".to_string()
                    );
                }
                // An i64 is signed, so the whole 64-bit range is written by giving all sixteen
                // digits: `0xFFFFFFFFFFFFFFFF` is -1, the same bits `bit_not(0)` produces. Parsed
                // as unsigned and reinterpreted, because that is what a bit pattern means — and it
                // is checked, so `0x1_0000_0000_0000_0000` is refused rather than wrapped.
                return match u64::from_str_radix(&digits, 16) {
                    Ok(v) => Ok(Token::Int(v as i64)),
                    Err(_) => Err(format!(
                        "0x{} does not fit in 64 bits — an Int holds sixteen hex digits",
                        digits
                    )),
                };
            }
        }
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
                    // A carriage return had no spelling at all until v0.0.176, which was an
                    // oversight rather than a decision: `\r` is standard in every language that has
                    // string escapes, and its absence meant a Burxt program could not PRODUCE the
                    // byte. `lib/os.bx`'s `os_byte_as_string` answered `"?"` for it, and
                    // `lib/json.bx` could not decode a `\r` inside a JSON string without losing it.
                    Some('r') => s.push('\r'),
                    Some('0') => s.push('\0'),
                    Some('{') => s.push('{'),
                    Some('}') => s.push('}'),
                    Some(other) if other != '\n' => {
                        return Err(format!(
                            "unknown escape `\\{}` — Burxt strings support \\\\, \\\", \
                             \\n, \\r, \\t, \\0, \\{{ and \\}}",
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
            // Spelled the way every comparison language spells it, which is the v0.0.98 test:
            // `const` is not a clipping of a longer word the way `fn` was of `function`, so
            // there is no fuller spelling to prefer. C, C++, C#, Rust, JavaScript, PHP and
            // TypeScript all say `const`; Java says `static final` and Python says nothing at
            // all. A reviewer reads this without decoding it, which is the whole rule.
            "const" => Token::Const,
            "print" => Token::Print,
            "print_error" => Token::PrintError,
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
            // C2. `public`, spelled out — every other name in this language is.
            "public" => Token::Public,
            "break" => Token::Break,
            "continue" => Token::Continue,
            "if" => Token::If,
            "else" => Token::Else,
            "true" => Token::True,
            "false" => Token::False,
            "class" => Token::Class,
            "private" => Token::Private,
            "region" => Token::Region,
            // `enum` stays clipped, and this is the reason rather than an oversight.
            //
            // It is short for "enumeration", so by the v0.0.98 rule it should be spelled out —
            // and the full word is WORSE on both counts: longer, and inaccurate, because a
            // Burxt enum is a sum type whose variants carry values, not an enumeration of
            // integers. `choice` would be accurate and is the honest alternative; `enum` wins
            // only because every language spells it this way and no reader expands it mentally.
            //
            // That is a weaker defence than `record` had over `struct`, where a better word
            // existed. Recorded so the next person weighs it rather than assuming it was
            // considered.
            "enum" => Token::Enum,
            "match" => Token::Match,
            "interface" => Token::Interface,
            "is" => Token::Is,
            "self" => Token::SelfKw,
            "implement" => Token::Impl,
            "implements" => Token::Implements,
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
            // The four sized C integers, roadmap A7. Lower-case and unprefixed because that is what
            // a C header and every systems language spell them: `uint8_t` is `u8` in Rust, Zig, Odin
            // and Swift's `UInt8`, and a Burxt-flavoured `CUInt8` would make a reader translate at
            // every FFI declaration. They are boundary-only, so the lower-case spelling never sits
            // beside a Burxt type where the case difference would look arbitrary.
            //
            // FOUR and not every width C has: these are the ones the roadmap's own callers need —
            // `i32` for `clock_gettime`, `u8` for a `dirent.d_name` byte, `u32`/`u64` for fixed-width
            // binary formats. `i8`/`i16`/`u16` cost one line each the day something wants them, and
            // adding a width nobody has asked for is a rule that compiles and enforces nothing.
            "i32" => Token::TyWidth { bits: 32, signed: true },
            "u8" => Token::TyWidth { bits: 8, signed: false },
            "u32" => Token::TyWidth { bits: 32, signed: false },
            "u64" => Token::TyWidth { bits: 64, signed: false },
            "CPointer" => Token::TyCPointer,
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
            "class",
            "one block holding a type's fields AND what you do with them, which is the thing \
             a struct declaration leaves you to assemble yourself",
        ),
        // `record` was the word until v0.0.148, and it was renamed for a reason a reader
        // deserves: a class held only fields, so everything you could DO with one lived
        // somewhere else. Writing the same point-of-sale in four languages made the cost
        // obvious — Python, PHP and Rust all put the two together, and Burxt could not.
        //
        // The name is honest about what this is and is not. A Burxt class has value semantics,
        // no inheritance, no hidden header and no constructor order to remember; it is nearer a
        // Swift struct or a Kotlin data class than a Java class. What it borrows from the word
        // is the part people actually want: fields, behaviour and privacy in one place.
        // `trait` was the word until v0.0.153. Renamed for the reason the whole
        // de-Rust-ification has: `interface` is what Java, C#, TypeScript, PHP and Go call
        // this, and the target reader is the 70% who write PHP and C#. An unfamiliar spelling
        // is something a REVIEWER has to stop and decode and an AGENT has to have memorised —
        // so familiarity is a safety property here, not a preference. See DESIGN.md.
        "trait" => (
            "interface",
            "a set of methods a type promises to have, which is what every language outside \
             Rust and Scala calls an interface",
        ),
        "record" => (
            "class",
            "a class holds its fields and its methods together, which is what `record` could \
             not do — and privacy needs a boundary to be private FROM",
        ),
        _ => return None,
    };
    Some(format!(
        "Burxt spells this `{}`, not `{}`: {}. Every keyword in this language is the word it \
         means — which is why `allocates` and `decreases` are not `alloc` and `dec`.",
        new, word, why
    ))
}
