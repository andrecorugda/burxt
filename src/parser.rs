//! Parser: tokens -> AST via straightforward recursive descent.
//!
//! Grammar:
//!   program := (struct | extern | fn | stmt)*
//!   struct  := "struct" IDENT "{" (param ",")* param? "}"
//!   extern  := "extern" "fn" IDENT "(" (param ("," param)*)? ")" "->" type ";"
//!   fn      := "fn" IDENT "(" (param ("," param)*)? ")" "->" type block
//!   param   := IDENT ":" type
//!   block   := "{" stmt* "}"
//!   stmt    := "let" "mut"? IDENT ":" type "=" expr ";"
//!            | IDENT ("." IDENT)* "=" expr ";"
//!            | "print" "(" expr ")" ";"
//!            | "return" expr ";"
//!            | "if" expr block ("else" (block | if-stmt))?
//!            | "while" expr block
//!   type    := "Int" | "Bool" | "String" | IDENT
//!            | "Decimal" "<" INT ("," rounding)? ">"
//!   rounding:= "RoundHalfEven" | "RoundHalfUp"
//!   expr    := additive (cmp additive)?          -- comparisons don't chain
//!   cmp     := "==" | "!=" | "<" | "<=" | ">" | ">="
//!   additive:= term (("+"|"-") term)*
//!   term    := factor (("*"|"/") factor)*
//!   factor  := primary ("." IDENT)*
//!   primary := INT | DECIMAL | STRING | "true" | "false" | IDENT
//!            | IDENT "(" (expr ("," expr)*)? ")"
//!            | IDENT "{" (IDENT ":" expr ",")* "}"   -- not in if/while conds
//!            | "(" expr ")"

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use crate::lexer::Token;

pub struct Parser {
    toks: Vec<Token>,
    /// Where each token came from, indexed alongside `toks`. Kept parallel
    /// rather than packed into the token so every `self.at(&Token::X)` in this
    /// file reads exactly as it did before spans existed.
    spans: Vec<Span>,
    pos: usize,
    /// The whole source, for quoting contract clauses back verbatim.
    src: String,
    /// Struct literals are not allowed directly in an if/while condition —
    /// `while count { ... }` must parse `{` as the loop body, not a literal.
    /// Parenthesizing re-enables them.
    allow_struct_lit: bool,
    /// The type parameters of the generic being parsed, so `parse_type` can tell `T`
    /// from a struct called `T`. Cleared when the declaration ends.
    type_params: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self::with_source(tokens, "")
    }

    /// The source is kept so a contract clause can be quoted back exactly as
    /// written when it fails at runtime. Spans alone are not enough: the message is
    /// baked into the compiled program, long after the source has gone.
    pub fn with_source(tokens: Vec<(Token, Span)>, src: &str) -> Self {
        let (toks, spans) = tokens.into_iter().unzip();
        Parser {
            toks,
            spans,
            pos: 0,
            allow_struct_lit: true,
            src: src.to_string(),
            type_params: Vec::new(),
        }
    }

    /// Is the current token the contextual word `word`?
    ///
    /// Contextual rather than reserved: these appear in exactly one position each, so
    /// recognising them there costs nothing and leaves the name free everywhere else.
    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Token::Ident(name) if name == word)
    }

    /// The source text a span covers, trimmed.
    fn text_of(&self, span: Span) -> String {
        let (a, b) = (span.start as usize, span.end as usize);
        if b <= self.src.len() && a < b {
            self.src[a..b].trim().to_string()
        } else {
            String::new()
        }
    }

    /// Wrap an expression with the source range it covers: from `start` to the
    /// end of the last token consumed.
    fn expr(&self, kind: ExprKind, start: u32) -> Expr {
        Expr { kind, span: Span { start, end: self.prev_end().max(start + 1) } }
    }

    /// The span of the token the parser is looking at. A parse error is always
    /// "this token is not what I needed", so this is where the caret belongs.
    fn span(&self) -> Span {
        self.spans.get(self.pos).copied().unwrap_or_default()
    }

    /// The end of the last consumed token — where a construct stops.
    fn prev_end(&self) -> u32 {
        if self.pos == 0 {
            return 0;
        }
        self.spans.get(self.pos - 1).map(|s| s.end).unwrap_or_default()
    }

    /// Parse, and on failure report WHERE the parser gave up.
    ///
    /// The message sites are left alone deliberately: a parse error surfaces
    /// immediately, so the token under the cursor at that moment IS the token the
    /// message is about. Threading a span through forty `format!` calls would add
    /// no information the position already carries.
    pub fn parse(mut self) -> Result<Program, Diagnostic> {
        match self.parse_program_inner() {
            Ok(p) => Ok(p),
            Err(message) => Err(Diagnostic::new(message, self.span())),
        }
    }

    fn parse_program_inner(&mut self) -> Result<Program, String> {
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut traits = Vec::new();
        let mut impls = Vec::new();
        let mut externs = Vec::new();
        let mut fns = Vec::new();
        let mut methods = Vec::new();
        let mut stmts = Vec::new();
        while !self.at(&Token::Eof) {
            if self.at(&Token::Struct) {
                structs.push(self.parse_struct()?);
            } else if self.at(&Token::Enum) {
                enums.push(self.parse_enum()?);
            } else if self.at(&Token::Trait) {
                traits.push(self.parse_trait()?);
            } else if self.at(&Token::Impl) {
                impls.push(self.parse_impl()?);
            } else if self.at(&Token::Extern) {
                externs.push(self.parse_extern()?);
            } else if self.at(&Token::Pure) {
                // `pure` only precedes a free function: a method cannot carry the
                // marker yet, and saying so beats a confusing parse error.
                if self.peek_at(1) != &Token::Fn {
                    return Err(format!(
                        "`pure` must be followed by `fn`, but found {}",
                        self.peek_at(1).describe()
                    ));
                }
                if self.peek_at(2) == &Token::LParen {
                    return Err(
                        "a method cannot be declared `pure` yet — the marker goes on \
                         a free function for now. Move the calculation into one, or \
                         drop `pure`."
                            .to_string(),
                    );
                }
                fns.push(self.parse_fn()?);
            } else if self.at(&Token::Fn) {
                // `fn (self: T) name(...)` is a method; `fn name(...)` is a
                // free function — the `(` right after `fn` is the tell.
                if self.peek_at(1) == &Token::LParen {
                    // Top level: there is no `impl` header to borrow a type from.
                    methods.push(self.parse_method(None)?);
                } else {
                    fns.push(self.parse_fn()?);
                }
            } else {
                stmts.push(self.parse_stmt()?);
            }
        }
        Ok(Program { structs, enums, traits, impls, externs, fns, methods, stmts })
    }

    // ---- helpers ----

    /// Peek `offset` tokens ahead without consuming (0 = current).
    fn peek_at(&self, offset: usize) -> &Token {
        self.toks.get(self.pos + offset).unwrap_or(&Token::Eof)
    }

    fn peek(&self) -> &Token {
        &self.toks[self.pos]
    }

    fn at(&self, t: &Token) -> bool {
        self.peek() == t
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, t: &Token) -> Result<(), String> {
        if self.at(t) {
            self.bump();
            Ok(())
        } else {
            Err(format!("expected {}, found {}", t.describe(), self.peek().describe()))
        }
    }

    // ---- structs ----

    fn parse_struct(&mut self) -> Result<StructDef, String> {
        let start = self.span().start;
        self.expect(&Token::Struct)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected a struct name after 'struct', found {}", other.describe())),
        };
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while !self.at(&Token::RBrace) {
            let fname = match self.bump() {
                Token::Ident(s) => s,
                other => return Err(format!("expected a field name in struct {}, found {}", name, other.describe())),
            };
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            // Accepted here only so the typechecker can explain WHY a field
            // cannot have one; "expected `}`" would not teach anything.
            let marshal = self.parse_marshal()?;
            fields.push(Param { name: fname, ty, marshal });
            if self.at(&Token::Comma) {
                self.bump(); // trailing comma allowed
            } else {
                break;
            }
        }
        self.expect(&Token::RBrace)?;
        Ok(StructDef { name, fields, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    /// `enum Name { Unit, WithPayload(Int, String), }`
    fn parse_enum(&mut self) -> Result<EnumDef, String> {
        let start = self.span().start;
        self.expect(&Token::Enum)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected an enum name after 'enum', found {}",
                    other.describe()
                ))
            }
        };
        let type_params = self.parse_type_params(&name)?;
        self.type_params = type_params.clone();
        self.expect(&Token::LBrace)?;
        let mut variants = Vec::new();
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err(format!("unclosed enum `{}`: expected `}}`", name));
            }
            let vname = match self.bump() {
                Token::Ident(s) => s,
                other => {
                    return Err(format!(
                        "expected a variant name in enum `{}`, found {}",
                        name,
                        other.describe()
                    ))
                }
            };
            let mut payload = Vec::new();
            if self.at(&Token::LParen) {
                self.bump();
                loop {
                    payload.push(self.parse_type()?);
                    if !self.more_in_list(&Token::RParen) {
                        break;
                    }
                }
                self.expect(&Token::RParen)?;
            }
            variants.push(Variant { name: vname, payload });
            if self.at(&Token::Comma) {
                self.bump(); // trailing comma allowed
            } else {
                break;
            }
        }
        self.expect(&Token::RBrace)?;
        self.type_params.clear();
        Ok(EnumDef { name, type_params, variants, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    // ---- traits and impls ----

    /// `trait Name { fn m(self) -> T   fn n(mut self, x: U) -> V }`
    /// Signatures only: no bodies, no fields, no default methods.
    fn parse_trait(&mut self) -> Result<TraitDef, String> {
        let start = self.span().start;
        self.expect(&Token::Trait)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a trait name after 'trait', found {}",
                    other.describe()
                ))
            }
        };
        self.expect(&Token::LBrace)?;
        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err(format!("unclosed trait `{}`: expected `}}`", name));
            }
            methods.push(self.parse_trait_sig(&name)?);
            // a separating semicolon is allowed but not required
            if self.at(&Token::Semicolon) {
                self.bump();
            }
        }
        self.expect(&Token::RBrace)?;
        Ok(TraitDef { name, methods, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    /// One signature inside a trait: `fn m(self) -> T`, or
    /// `fn m(mut self, extra: U) -> V`. The receiver has no type — it is
    /// whichever type implements the trait.
    fn parse_trait_sig(&mut self, trait_name: &str) -> Result<TraitSig, String> {
        self.expect(&Token::Fn)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a method name in trait `{}`, found {}",
                    trait_name,
                    other.describe()
                ))
            }
        };
        self.expect(&Token::LParen)?;
        let receiver_mut = if self.at(&Token::Mut) {
            self.bump();
            true
        } else {
            false
        };
        if !self.at(&Token::SelfKw) {
            return Err(format!(
                "every trait method takes `self` first: write `fn {}({}self, ...)`, \
                 found {}",
                name,
                if receiver_mut { "mut " } else { "" },
                self.peek().describe()
            ));
        }
        self.bump();
        let mut params = Vec::new();
        while self.at(&Token::Comma) {
            self.bump();
            let pname = match self.bump() {
                Token::Ident(s) => s,
                other => {
                    return Err(format!("expected a parameter name, found {}", other.describe()))
                }
            };
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            params.push(Param { name: pname, ty, marshal: None });
        }
        self.expect(&Token::RParen)?;
        if !self.at(&Token::Arrow) {
            return Err(format!(
                "expected `->` and a return type for `{}.{}` (every Burxt function \
                 returns a value), found {}",
                trait_name,
                name,
                self.peek().describe()
            ));
        }
        self.bump();
        let ret = self.parse_type()?;
        Ok(TraitSig { name, receiver_mut, params, ret })
    }

    /// `impl Trait for Type { <method definitions> }`
    fn parse_impl(&mut self) -> Result<ImplBlock, String> {
        let start = self.span().start;
        self.expect(&Token::Impl)?;
        let trait_name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a trait name after 'impl', found {}",
                    other.describe()
                ))
            }
        };
        if !self.at(&Token::For) {
            return Err(format!(
                "expected `for` in `impl {} for Type`, found {}",
                trait_name,
                self.peek().describe()
            ));
        }
        self.bump();
        let type_name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a type name in `impl {} for ...`, found {}",
                    trait_name,
                    other.describe()
                ))
            }
        };
        self.expect(&Token::LBrace)?;
        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err(format!(
                    "unclosed `impl {} for {}`: expected `}}`",
                    trait_name, type_name
                ));
            }
            methods.push(self.parse_method(Some(&type_name))?);
        }
        self.expect(&Token::RBrace)?;
        Ok(ImplBlock { trait_name, type_name, methods, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    // ---- functions ----

    fn parse_extern(&mut self) -> Result<ExternFn, String> {
        let start = self.span().start;
        self.expect(&Token::Extern)?;
        self.expect(&Token::Fn)?;
        let (name, type_params, params, ret) = self.parse_fn_signature()?;
        if !type_params.is_empty() {
            // C has no notion of a type parameter, and a monomorphised C symbol is a
            // symbol that does not exist. See spec/M7-GENERICS.md §2.
            return Err(format!(
                "`extern fn {}` cannot be generic: C has no type parameters, and there \
                 would be no symbol to link against.",
                name
            ));
        }
        self.expect(&Token::Semicolon)?;
        Ok(ExternFn { name, params, ret, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    /// Contract clauses sit between the signature and the body, where a reader
    /// looks for what a function demands and promises. Shared by functions and
    /// methods, so the two can never drift.
    fn parse_contracts(
        &mut self,
    ) -> Result<(Vec<Contract>, Vec<Contract>, Option<Contract>), String> {
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        let mut decreases = None;
        while self.at_word("requires") || self.at_word("ensures") || self.at_word("decreases")
        {
            // Which clause it is, decided before the word is consumed.
            let which = match self.peek() {
                Token::Ident(name) => name.clone(),
                _ => unreachable!("at_word matched a non-identifier"),
            };
            self.bump();
            let start = self.span().start;
            // A condition, not an expression: a `{` after it opens the BODY, so
            // struct literals are off exactly as in an `if`.
            let cond = self.parse_cond()?;
            let span = Span { start, end: self.prev_end().max(start + 1) };
            let clause = Contract { cond, text: self.text_of(span), span };
            match which.as_str() {
                "requires" => requires.push(clause),
                "ensures" => ensures.push(clause),
                _ => {
                    if decreases.is_some() {
                        return Err(
                            "a function may have one `decreases` measure. Two would be \
                             a lexicographic measure, which is not built."
                                .to_string(),
                        );
                    }
                    decreases = Some(clause);
                }
            }
        }
        Ok((requires, ensures, decreases))
    }

    fn parse_fn(&mut self) -> Result<FnDef, String> {
        let start = self.span().start;
        // `pure fn ...` — a prefix, because it is a statement about the whole
        // function rather than about its result.
        let is_pure = self.at(&Token::Pure);
        if is_pure {
            self.bump();
        }
        self.expect(&Token::Fn)?;
        let (name, type_params, params, ret) = self.parse_fn_signature()?;
        // `-> T allocates` reads as what it is: returns a T, and allocates.
        let allocates = self.at_word("allocates");
        if allocates {
            self.bump();
        }
        let (requires, ensures, decreases) = self.parse_contracts()?;
        let body = self.parse_block()?;
        // The parameters are only in scope for this signature and body.
        self.type_params.clear();
        Ok(FnDef { name, type_params, params, ret, allocates, is_pure, requires, ensures, decreases, body, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    /// `fn (self: Type) name(params) -> ret { body }`, or `fn (mut self: ...)`
    /// for a mutating method.
    fn parse_method(&mut self, owner: Option<&str>) -> Result<MethodDef, String> {
        let start = self.span().start;
        self.expect(&Token::Fn)?;
        self.expect(&Token::LParen)?;
        let receiver_mut = if self.at(&Token::Mut) {
            self.bump();
            true
        } else {
            false
        };
        if !self.at(&Token::SelfKw) {
            return Err(format!(
                "expected `self` in the receiver clause `fn ({}self: Type)`, found {}",
                if receiver_mut { "mut " } else { "" },
                self.peek().describe()
            ));
        }
        self.bump();
        // `fn (self) name()` inside an `impl` — the header already said which type, so
        // repeating it on every method buys nothing. `owner` is that type when we are
        // inside one; outside, there is nothing to fall back on and the annotation stays
        // required, because there the type genuinely is not known.
        let receiver = if self.at(&Token::RParen) {
            match owner {
                Some(t) => t.to_string(),
                None => {
                    return Err(
                        "`fn (self)` needs the type: outside an `impl` block nothing says \
                         which one. Write `fn (self: Type) name(...)`, or put the method \
                         in `impl Trait for Type { ... }`, where the header says it once."
                            .to_string(),
                    )
                }
            }
        } else {
            self.expect(&Token::Colon)?;
            match self.bump() {
                Token::Ident(s) => s,
                other => {
                    return Err(format!(
                        "expected a struct name after `self:`, found {}",
                        other.describe()
                    ))
                }
            }
        };
        self.expect(&Token::RParen)?;
        let (name, type_params, params, ret) = self.parse_fn_signature()?;
        if !type_params.is_empty() {
            // A method may use its TYPE's parameters; its own are a later slice, and a
            // parameter list that silently did nothing would be worse than a refusal.
            // See spec/M7-GENERICS.md §3.
            return Err(format!(
                "`{}` declares its own type parameters, and a method may not yet: it may \
                 use the parameters of the type it is on. Move it to a free function.",
                name
            ));
        }
        let allocates = self.at_word("allocates");
        if allocates {
            self.bump();
        }
        let (requires, ensures, decreases) = self.parse_contracts()?;
        if let Some(d) = &decreases {
            let _ = d;
            return Err(
                "`decreases` on a method is not built yet — the measure goes on a free \
                 function for now."
                    .to_string(),
            );
        }
        let body = self.parse_block()?;
        Ok(MethodDef { receiver, receiver_mut, name, params, ret, allocates, requires, ensures, body, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    /// `as <marshaller>` declares how a value crosses a foreign boundary.
    /// Parsed everywhere a type can appear, and refused with an explanation
    /// wherever it makes no sense — a parse error would say `expected }` and
    /// teach nothing.
    fn parse_marshal(&mut self) -> Result<Option<Marshal>, String> {
        if !self.at(&Token::As) {
            return Ok(None);
        }
        self.bump();
        match self.bump() {
            Token::Ident(word) if word == "scaled" => Ok(Some(Marshal::Scaled)),
            other => Err(format!(
                "unknown boundary marshaller {} — the only one is `scaled`, as in \
                 `amount: Decimal<2> as scaled`, which passes the exact unscaled \
                 integer.",
                other.describe()
            )),
        }
    }

    /// `<T>` or `<T, U>` after a name, or nothing. Refused: an empty list, a duplicate,
    /// and a parameter whose name is a declared type's — each of those is a program that
    /// means two things.
    /// Is there another element in this comma-separated list?
    ///
    /// A **trailing comma is allowed everywhere**, so adding a field, a parameter or an
    /// argument is a one-line diff rather than a two-line one. It buys nothing to refuse
    /// and it costs a reader nothing to allow, which is the whole test.
    fn more_in_list(&mut self, closer: &Token) -> bool {
        if !self.at(&Token::Comma) {
            return false;
        }
        self.bump();
        !self.at(closer)
    }

    fn parse_type_params(&mut self, owner: &str) -> Result<Vec<String>, String> {
        if !self.at(&Token::Lt) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut names: Vec<String> = Vec::new();
        loop {
            match self.bump() {
                Token::Ident(s) => {
                    if names.contains(&s) {
                        return Err(format!(
                            "`{}` declares the type parameter `{}` twice",
                            owner, s
                        ));
                    }
                    names.push(s);
                }
                other => {
                    return Err(format!(
                        "expected a type parameter name in `{}<...>`, found {}",
                        owner,
                        other.describe()
                    ))
                }
            }
            if !self.more_in_list(&Token::Gt) {
                break;
            }
        }
        self.expect(&Token::Gt)?;
        if names.is_empty() {
            return Err(format!("`{}<>` declares nothing — drop the angle brackets", owner));
        }
        Ok(names)
    }

    fn parse_fn_signature(&mut self) -> Result<(String, Vec<String>, Vec<Param>, Type), String> {
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected a function name after 'fn', found {}", other.describe())),
        };
        // `fn name<T, U>(...)`. Recorded on the parser so `parse_type` can tell a type
        // parameter from a struct name — which is the only place that distinction is
        // visible, since both are spelled as a bare identifier.
        let type_params = self.parse_type_params(&name)?;
        self.type_params = type_params.clone();
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        if !self.at(&Token::RParen) {
            loop {
                let pname = match self.bump() {
                    Token::Ident(s) => s,
                    other => return Err(format!("expected a parameter name, found {}", other.describe())),
                };
                self.expect(&Token::Colon)?;
                let ty = self.parse_type()?;
                let marshal = self.parse_marshal()?;
                params.push(Param { name: pname, ty, marshal });
                if !self.more_in_list(&Token::RParen) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen)?;
        if !self.at(&Token::Arrow) {
            return Err(format!(
                "expected `->` and a return type after fn {}'s parameter list \
                 (every Burxt function returns a value), found {}",
                name,
                self.peek().describe()
            ));
        }
        self.bump();
        let ret = self.parse_type()?;
        Ok((name, type_params, params, ret))
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect(&Token::LBrace)?;
        let mut stmts = Vec::new();
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err("unclosed block: expected `}`".to_string());
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(stmts)
    }

    // ---- statements ----

    /// Every statement is wrapped with the source range it covers, here, once —
    /// so no individual statement parser has to remember to carry a span.
    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        let start = self.span().start;
        let kind = self.parse_stmt_kind()?;
        Ok(Stmt { kind, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    fn parse_stmt_kind(&mut self) -> Result<StmtKind, String> {
        match self.peek() {
            Token::Let => self.parse_let(),
            Token::Print => self.parse_print(),
            Token::Break => {
                self.bump();
                self.expect(&Token::Semicolon)?;
                Ok(StmtKind::Break)
            }
            Token::Continue => {
                self.bump();
                self.expect(&Token::Semicolon)?;
                Ok(StmtKind::Continue)
            }
            Token::Return => self.parse_return(),
            Token::If => self.parse_if(),
            Token::While => self.parse_while(),
            Token::For => self.parse_for(),
            Token::Match => self.parse_match(),
            Token::Region => self.parse_region(),
            Token::Ident(_) | Token::SelfKw => self.parse_assign(),
            other => Err(format!("expected statement, found {}", other.describe())),
        }
    }

    /// A statement starting with an identifier (or `self`) is one of:
    ///   name = value;                  assignment
    ///   name[index] = value;           element assignment
    ///   name.a.b = value;              field assignment
    ///   name(args);                    a call kept for its side effect
    ///   name.a.b.method(args);         a method call kept for its side effect
    /// The dot-chain is walked once; hitting `(` after a segment means that
    /// segment is a method name, not a field, and everything read so far
    /// becomes the call's base expression.
    /// The `=` of an assignment, or one of `+= -= *=`.
    ///
    /// A compound assignment is expanded here into `target = target <op> value`, so nothing
    /// downstream learns a new statement kind: no typecheck rule, no lowering, and the
    /// scale and contract rules apply exactly as they do to the long form. `x += 1` on a
    /// Decimal obeys the same "scales must match" refusal, because it IS the long form by
    /// the time anyone checks it.
    fn parse_assign_op(&mut self) -> Result<Option<BinOp>, String> {
        let op = match self.peek() {
            Token::Equals => None,
            Token::PlusEq => Some(BinOp::Add),
            Token::MinusEq => Some(BinOp::Sub),
            Token::StarEq => Some(BinOp::Mul),
            other => {
                return Err(format!("expected `=`, `+=`, `-=` or `*=`, found {}", other.describe()))
            }
        };
        self.bump();
        Ok(op)
    }

    /// `target <op>= value` becomes `target = target <op> value`.
    fn compound(&mut self, op: Option<BinOp>, target: Expr, value: Expr, start: u32) -> Expr {
        match op {
            None => value,
            Some(op) => self.expr(
                ExprKind::Binary { op, lhs: Box::new(target), rhs: Box::new(value) },
                start,
            ),
        }
    }

    fn parse_assign(&mut self) -> Result<StmtKind, String> {
        let start = self.span().start;
        let name = match self.bump() {
            Token::Ident(s) => s,
            Token::SelfKw => "self".to_string(),
            other => return Err(format!("expected identifier, found {}", other.describe())),
        };

        if self.at(&Token::LParen) {
            let args = self.parse_call_args()?;
            self.expect(&Token::Semicolon)?;
            return Ok(StmtKind::ExprStmt(self.expr(ExprKind::Call { name, args }, start)));
        }

        if self.at(&Token::LBracket) {
            self.bump();
            let index = self.parse_expr()?;
            self.expect(&Token::RBracket)?;
            let op = self.parse_assign_op()?;
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            let value = if op.is_some() {
                let base = self.expr(ExprKind::Var(name.clone()), start);
                let read = self.expr(
                    ExprKind::Index { base: Box::new(base), index: Box::new(index.clone()) },
                    start,
                );
                self.compound(op, read, value, start)
            } else {
                value
            };
            return Ok(StmtKind::AssignIndex { name, index, value });
        }

        let mut path = Vec::new();
        while self.at(&Token::Dot) {
            self.bump();
            let seg = match self.bump() {
                Token::Ident(f) => f,
                other => return Err(format!("expected a field name after '.', found {}", other.describe())),
            };
            if self.at(&Token::LParen) {
                let args = self.parse_call_args()?;
                self.expect(&Token::Semicolon)?;
                let mut base = self.expr(ExprKind::Var(name), start);
                for f in path {
                    base = self.expr(ExprKind::Field { base: Box::new(base), field: f }, start);
                }
                return Ok(StmtKind::ExprStmt(
                    self.expr(ExprKind::MethodCall { base: Box::new(base), method: seg, args }, start),
                ));
            }
            path.push(seg);
        }

        if self.at(&Token::LBracket) && !path.is_empty() {
            self.bump();
            let index = self.parse_expr()?;
            self.expect(&Token::RBracket)?;
            let op = self.parse_assign_op()?;
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            let value = if op.is_some() {
                let read = self.read_path(&name, &path, Some(index.clone()), start);
                self.compound(op, read, value, start)
            } else {
                value
            };
            return Ok(StmtKind::AssignFieldIndex { name, path, index, value });
        }
        let op = self.parse_assign_op()?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        let value = if op.is_some() {
            let read = self.read_path(&name, &path, None, start);
            self.compound(op, read, value, start)
        } else {
            value
        };
        if path.is_empty() {
            Ok(StmtKind::Assign { name, value })
        } else {
            Ok(StmtKind::AssignField { name, path, value })
        }
    }

    /// The expression that READS what an assignment writes: `self.total`, `xs[i]`,
    /// `a.b.c[i]`. A compound assignment needs both halves, and building the read from the
    /// same pieces is what keeps `x += 1` exactly equal to `x = x + 1`.
    fn read_path(
        &mut self,
        name: &str,
        path: &[String],
        index: Option<Expr>,
        start: u32,
    ) -> Expr {
        let mut base = self.expr(ExprKind::Var(name.to_string()), start);
        for f in path {
            base = self.expr(ExprKind::Field { base: Box::new(base), field: f.clone() }, start);
        }
        match index {
            Some(i) => {
                self.expr(ExprKind::Index { base: Box::new(base), index: Box::new(i) }, start)
            }
            None => base,
        }
    }

    /// Parse a parenthesized, comma-separated argument list, including the
    /// parens: `(a, b, c)`.
    fn parse_call_args(&mut self) -> Result<Vec<Expr>, String> {
        self.expect(&Token::LParen)?;
        let mut args = Vec::new();
        if !self.at(&Token::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if !self.more_in_list(&Token::RParen) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen)?;
        Ok(args)
    }

    /// `match value { Variant => { .. }  Other(a, b) => { .. } }`
    /// Patterns are UNQUALIFIED: the matched value's type already says which
    /// enum it is, so repeating it would be noise.
    fn parse_match(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::Match)?;
        let value = self.parse_cond()?; // no struct literal before the `{`
        self.expect(&Token::LBrace)?;
        let mut arms = Vec::new();
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err("unclosed `match`: expected `}`".to_string());
            }
            let variant = match self.bump() {
                Token::Ident(s) => s,
                // `_` would lex as an identifier, so a wildcard cannot reach
                // here as a distinct token; every other token is a real slip.
                other => {
                    return Err(format!(
                        "expected a variant name to match on, found {}",
                        other.describe()
                    ))
                }
            };
            let mut bindings = Vec::new();
            if self.at(&Token::LParen) {
                self.bump();
                loop {
                    match self.bump() {
                        Token::Ident(b) => bindings.push(b),
                        other => {
                            return Err(format!(
                                "expected a name for `{}`'s payload, found {}",
                                variant,
                                other.describe()
                            ))
                        }
                    }
                    if !self.more_in_list(&Token::RParen) {
                        break;
                    }
                }
                self.expect(&Token::RParen)?;
            }
            if !self.at(&Token::FatArrow) {
                return Err(format!(
                    "expected `=>` after the pattern `{}`, found {}",
                    variant,
                    self.peek().describe()
                ));
            }
            self.bump();
            let body = self.parse_block()?;
            arms.push(MatchArm { variant, bindings, body });
            if self.at(&Token::Comma) {
                self.bump(); // a separating comma is allowed
            }
        }
        self.expect(&Token::RBrace)?;
        Ok(StmtKind::Match { value, arms })
    }

    /// `region name { ... }`
    fn parse_region(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::Region)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a name for the region, as in `region tx {{ ... }}`, \
                     found {}",
                    other.describe()
                ))
            }
        };
        let body = self.parse_block()?;
        Ok(StmtKind::Region { name, body })
    }

    /// `for x in xs { body }`. See spec/M10-ERGONOMICS.md §1b.
    fn parse_for(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::For)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a name after `for`, found {} — `for x in xs {{ ... }}`",
                    other.describe()
                ))
            }
        };
        self.expect(&Token::In)?;
        let iterable = self.parse_cond()?;
        // The loop reads the iterable once per element, so anything with a cost or an
        // effect would pay it per pass. A binding and a field path are free to re-read.
        if !matches!(iterable.kind, ExprKind::Var(_) | ExprKind::Field { .. }) {
            return Err(format!(
                "`for` iterates a named array, and this is {}: its result would be \
                 recomputed on every pass. Bind it first — `let items = ...;` — and \
                 iterate that.",
                describe_iterable(&iterable.kind)
            ));
        }
        let body = self.parse_block()?;
        Ok(StmtKind::For { name, iterable, body })
    }

    fn parse_while(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::While)?;
        let cond = self.parse_cond()?;
        let body = self.parse_block()?;
        Ok(StmtKind::While { cond, body })
    }

    /// Parse an if/while condition: struct literals are disabled so the `{`
    /// after the condition always starts the block. Parentheses re-enable.
    fn parse_cond(&mut self) -> Result<Expr, String> {
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = false;
        let result = self.parse_expr();
        self.allow_struct_lit = saved;
        result
    }

    fn parse_return(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::Return)?;
        // `return tail f(x)` asks for a guaranteed tail call. It reads as what
        // it is at the call site, which is the whole point: a reader can see
        // that this call does not grow the stack.
        let tail = self.at(&Token::Tail);
        if tail {
            self.bump();
        }
        let e = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        if tail {
            if !matches!(e.kind, ExprKind::Call { .. }) {
                return Err(
                    "`return tail` must be followed by a call — a tail call is a \
                     call that replaces this frame, so there has to be one."
                        .to_string(),
                );
            }
            return Ok(StmtKind::TailReturn(e));
        }
        Ok(StmtKind::Return(e))
    }

    fn parse_if(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::If)?;
        let cond = self.parse_cond()?;
        let then_block = self.parse_block()?;
        let else_block = if self.at(&Token::Else) {
            self.bump();
            if self.at(&Token::If) {
                // `else if ...` chains as an else-block holding one if-statement.
                // Routed through parse_stmt so the nested `if` gets its own span.
                Some(vec![self.parse_stmt()?])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(StmtKind::If { cond, then_block, else_block })
    }

    fn parse_let(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::Let)?;
        let mutable = if self.at(&Token::Mut) {
            self.bump();
            true
        } else {
            false
        };
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected identifier after 'let', found {}", other.describe())),
        };
        // The annotation is optional. `let count = 0;` takes its type from the value —
        // and only here: a signature still says what it takes and what it answers with.
        let declared = if self.at(&Token::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Token::Equals)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(StmtKind::Let { name, mutable, declared, value })
    }

    fn parse_print(&mut self) -> Result<StmtKind, String> {
        self.expect(&Token::Print)?;
        self.expect(&Token::LParen)?;
        let e = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Semicolon)?;
        Ok(StmtKind::Print(e))
    }

    // ---- types ----

    /// Close a `Decimal<..>` type. Written as its own method because `>=`
    /// lexes as one token: in `let x: Decimal<2>= 1.00;` the `>` closes the
    /// type and the `=` belongs to the let — so we split it here rather than
    /// making the language whitespace-sensitive.
    fn expect_type_close(&mut self) -> Result<(), String> {
        if self.at(&Token::Gt) {
            self.bump();
            Ok(())
        } else if self.at(&Token::Ge) {
            self.toks[self.pos] = Token::Equals;
            Ok(())
        } else {
            Err(format!("expected `>` to close Decimal<..>, found {}", self.peek().describe()))
        }
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        match self.bump() {
            Token::TyInt => Ok(Type::Int),
            Token::TyBool => Ok(Type::Bool),
            Token::TyString => Ok(Type::String),
            Token::TyCInt => Ok(Type::CInt),
            Token::TyCDouble => Ok(Type::CDouble),
            Token::TyDecimal => {
                self.expect(&Token::Lt)?;
                let scale = match self.bump() {
                    Token::Int(n) if (0..=18).contains(&n) => n as u32,
                    Token::Int(n) => {
                        return Err(format!(
                            "Decimal scale must be between 0 and 18, but this is {} — \
                             a scaled i64 holds at most 18 fractional digits",
                            n
                        ))
                    }
                    other => return Err(format!("expected non-negative scale in Decimal<..>, found {}", other.describe())),
                };
                // Optional rounding contract: Decimal<2, RoundHalfEven>.
                let rounding = if self.at(&Token::Comma) {
                    self.bump();
                    match self.bump() {
                        Token::RoundHalfEven => Some(Rounding::HalfEven),
                        Token::RoundHalfUp => Some(Rounding::HalfUp),
                        other => {
                            return Err(format!(
                                "expected a rounding mode (RoundHalfEven or RoundHalfUp) \
                                 after the comma in Decimal<..>, found {}",
                                other.describe()
                            ))
                        }
                    }
                } else {
                    None
                };
                self.expect_type_close()?;
                Ok(Type::Decimal { scale, rounding })
            }
            // A bare identifier is a struct type; whether it exists is the
            // typechecker's question, so use-before-declaration works.
            // A bare identifier is a struct or enum name — unless the generic being
            // parsed declared it as a type parameter, which is the only thing that tells
            // `T` from a struct called `T`.
            Token::Ident(name) => {
                if self.type_params.contains(&name) {
                    return Ok(Type::Param(name));
                }
                // `Option<Int>` — a generic applied. `<` after a type name can only mean
                // this, so no lookahead beyond the one token is needed.
                if self.at(&Token::Lt) {
                    self.bump();
                    let mut args = Vec::new();
                    loop {
                        args.push(self.parse_type()?);
                        if !self.more_in_list(&Token::Gt) {
                            break;
                        }
                    }
                    self.expect(&Token::Gt)?;
                    return Ok(Type::Generic { name, args });
                }
                Ok(Type::Named(name))
            }
            // `dyn Trait` — the only syntax that asks for dynamic dispatch.
            // If you never write `dyn`, you never pay for a vtable.
            Token::Dyn => match self.bump() {
                Token::Ident(name) => Ok(Type::Dyn(name)),
                other => Err(format!(
                    "expected a trait name after `dyn`, found {}",
                    other.describe()
                )),
            },
            // [T; N] — fixed-size array
            Token::LBracket => {
                let elem = self.parse_type()?;
                // `[T]` is growable and region-allocated; `[T; N]` is fixed.
                if self.at(&Token::RBracket) {
                    self.bump();
                    return Ok(Type::Slice(Box::new(elem)));
                }
                self.expect(&Token::Semicolon)?;
                let len = match self.bump() {
                    Token::Int(0) => {
                        return Err("an array must hold at least one value".to_string())
                    }
                    Token::Int(n) if (1..=65536).contains(&n) => n as u32,
                    Token::Int(n) => {
                        return Err(format!(
                            "arrays live on the stack; [T; N] is capped at 65536 \
                             values for now, but this is {}",
                            n
                        ))
                    }
                    other => {
                        return Err(format!(
                            "expected the array length after `;` in [T; N], found {}",
                            other.describe()
                        ))
                    }
                };
                self.expect(&Token::RBracket)?;
                Ok(Type::Array { elem: Box::new(elem), len })
            }
            other => Err(format!("expected a type, found {}", other.describe())),
        }
    }

    // ---- expressions (precedence climbing) ----

    /// Comparison is the loosest level, and it deliberately does not chain:
    /// `a < b < c` is not a Burxt expression (and would be a type error anyway
    /// — a Bool has no order).
    /// The loosest level: `||`, then `&&`, then comparison. Both are
    /// left-associative and both short-circuit.
    fn parse_expr(&mut self) -> Result<Expr, String> {
        let start = self.span().start;
        let mut lhs = self.parse_and()?;
        while self.at(&Token::PipePipe) {
            self.bump();
            let rhs = self.parse_and()?;
            lhs = self.expr(
                ExprKind::Logical {
                    op: LogicalOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                start,
            );
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let start = self.span().start;
        let mut lhs = self.parse_comparison()?;
        while self.at(&Token::AmpAmp) {
            self.bump();
            let rhs = self.parse_comparison()?;
            lhs = self.expr(
                ExprKind::Logical {
                    op: LogicalOp::And,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                start,
            );
        }
        Ok(lhs)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let start = self.span().start;
        let lhs = self.parse_additive()?;
        let op = match self.peek() {
            Token::EqEq => CmpOp::Eq,
            Token::NotEq => CmpOp::Ne,
            Token::Lt => CmpOp::Lt,
            Token::Le => CmpOp::Le,
            Token::Gt => CmpOp::Gt,
            Token::Ge => CmpOp::Ge,
            _ => return Ok(lhs),
        };
        self.bump();
        let rhs = self.parse_additive()?;
        if matches!(
            self.peek(),
            Token::EqEq | Token::NotEq | Token::Lt | Token::Le | Token::Gt | Token::Ge
        ) {
            return Err(
                "comparisons do not chain — `a < b < c` is not a Burxt expression. \
                 Write the two comparisons separately."
                    .to_string(),
            );
        }
        Ok(self.expr(ExprKind::Compare { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, start))
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let start = self.span().start;
        let mut lhs = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_term()?;
            lhs = self.expr(ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, start);
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let start = self.span().start;
        let mut lhs = self.parse_factor()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_factor()?;
            lhs = self.expr(ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, start);
        }
        Ok(lhs)
    }

    /// A factor is a primary followed by any chain of `.field` accesses and
    /// `.method(args)` calls, optionally negated:
    /// `-item.price` is Neg(Field(item, price)).
    fn parse_factor(&mut self) -> Result<Expr, String> {
        let start = self.span().start;
        if self.at(&Token::Minus) {
            self.bump();
            let e = self.parse_factor()?;
            return Ok(self.expr(ExprKind::Neg(Box::new(e)), start));
        }
        if self.at(&Token::Bang) {
            self.bump();
            let e = self.parse_factor()?;
            return Ok(self.expr(ExprKind::Not(Box::new(e)), start));
        }
        let mut e = self.parse_primary()?;
        loop {
            if self.at(&Token::LBracket) {
                self.bump();
                let index = self.parse_expr()?;
                self.expect(&Token::RBracket)?;
                e = self.expr(ExprKind::Index { base: Box::new(e), index: Box::new(index) }, start);
                continue;
            }
            if !self.at(&Token::Dot) {
                break;
            }
            self.bump();
            let name = match self.bump() {
                Token::Ident(f) => f,
                other => return Err(format!("expected a field or method name after '.', found {}", other.describe())),
            };
            if self.at(&Token::LParen) {
                self.bump();
                let mut args = Vec::new();
                if !self.at(&Token::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if !self.more_in_list(&Token::RParen) {
                            break;
                        }
                    }
                }
                self.expect(&Token::RParen)?;
                e = self.expr(ExprKind::MethodCall { base: Box::new(e), method: name, args }, start);
            } else {
                e = self.expr(ExprKind::Field { base: Box::new(e), field: name }, start);
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let start = self.span().start;
        // A parenthesised expression keeps ITS OWN span — `(a + b)` should hover
        // and underline as `a + b`, not as the parentheses.
        if self.at(&Token::LParen) {
            self.bump();
            let saved = self.allow_struct_lit;
            self.allow_struct_lit = true;
            let e = self.parse_expr();
            self.allow_struct_lit = saved;
            let e = e?;
            self.expect(&Token::RParen)?;
            return Ok(e);
        }
        let kind = self.parse_primary_kind()?;
        Ok(self.expr(kind, start))
    }

    fn parse_primary_kind(&mut self) -> Result<ExprKind, String> {
        match self.bump() {
            Token::Int(n) => Ok(ExprKind::IntLit(n)),
            Token::Decimal(unscaled, scale) => Ok(ExprKind::DecimalLit { unscaled, scale }),
            Token::True => Ok(ExprKind::BoolLit(true)),
            Token::False => Ok(ExprKind::BoolLit(false)),
            Token::Str(s) => Ok(ExprKind::StrLit(s)),
            Token::InterpStr(parts) => {
                // Each `{...}` was captured as source text; parse it now, so an
                // interpolated expression obeys exactly the same grammar and
                // type rules as one written outside a string.
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        crate::lexer::StrPart::Lit(text) => out.push(InterpPart::Lit(text)),
                        crate::lexer::StrPart::Expr(src) => {
                            // The fragment is lexed on its own, so its offsets are
                            // relative to the fragment, not the file. Only the
                            // message travels out; the caret lands on the string
                            // literal, which is the right place until fragment
                            // offsets are recorded (see `StrPart`).
                            let toks = crate::lexer::Lexer::new(&src).tokenize().map_err(|e| {
                                format!("in the interpolation `{{{}}}`: {}", src.trim(), e.message)
                            })?;
                            let mut sub = Parser::new(toks);
                            let e = sub.parse_expr().map_err(|e| {
                                format!("in the interpolation `{{{}}}`: {}", src.trim(), e)
                            })?;
                            if !sub.at(&Token::Eof) {
                                return Err(format!(
                                    "in the interpolation `{{{}}}`: expected one \
                                     expression, but found more after it",
                                    src.trim()
                                ));
                            }
                            out.push(InterpPart::Expr(e));
                        }
                    }
                }
                Ok(ExprKind::InterpStr(out))
            }
            // `self` reads as a plain variable in expression position — its
            // meaning (and mutability) comes from the receiver clause, not
            // from special-casing here.
            Token::SelfKw => Ok(ExprKind::Var("self".to_string())),
            Token::Ident(s) => {
                if self.at(&Token::LParen) {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at(&Token::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if !self.more_in_list(&Token::RParen) {
                                break;
                            }
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(ExprKind::Call { name: s, args })
                } else if self.at(&Token::LBrace) && self.allow_struct_lit {
                    self.bump();
                    let mut fields = Vec::new();
                    while !self.at(&Token::RBrace) {
                        let fname = match self.bump() {
                            Token::Ident(f) => f,
                            other => {
                                return Err(format!(
                                    "expected a field name in `{} {{ ... }}`, found {}",
                                    s, other.describe()
                                ))
                            }
                        };
                        // `P { x, y }` is shorthand for `P { x: x, y: y }`: a field taking
                        // its value from a variable of the same name. `Order { subtotal:
                        // subtotal, country: country }` says nothing twice, and this is the
                        // one place a value can be filled in without inferring a TYPE.
                        let value = if self.at(&Token::Comma) || self.at(&Token::RBrace) {
                            let at = self.span().start;
                            self.expr(ExprKind::Var(fname.clone()), at)
                        } else {
                            self.expect(&Token::Colon)?;
                            self.parse_expr()?
                        };
                        fields.push((fname, value));
                        if self.at(&Token::Comma) {
                            self.bump(); // trailing comma allowed
                        } else {
                            break;
                        }
                    }
                    self.expect(&Token::RBrace)?;
                    Ok(ExprKind::StructLit { name: s, fields })
                } else {
                    // `name[i]` is handled by the postfix loop, so a bare name
                    // is all that is left here.
                    Ok(ExprKind::Var(s))
                }
            }
            Token::LBracket => {
                let mut elems = Vec::new();
                while !self.at(&Token::RBracket) {
                    elems.push(self.parse_expr()?);
                    if self.at(&Token::Comma) {
                        self.bump(); // trailing comma allowed
                    } else {
                        break;
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(ExprKind::ArrayLit(elems))
            }
            other => Err(format!("expected an expression, found {}", other.describe())),
        }
    }
}

/// What kind of thing an unusable `for` iterable is, for the message that refuses it.
fn describe_iterable(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Call { .. } => "a call",
        ExprKind::MethodCall { .. } => "a method call",
        ExprKind::Index { .. } => "an element",
        ExprKind::ArrayLit(_) => "a literal",
        ExprKind::Binary { .. } => "an expression",
        _ => "not a name",
    }
}
