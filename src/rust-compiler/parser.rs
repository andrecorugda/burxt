//! Parser: tokens -> AST via straightforward recursive descent.
//!
//! Grammar:
//!   program := (struct | extern | fn | stmt)*
//!   class   := "class" IDENT "{" (field ",")* field? method* "}"
//!   extern  := "external" "function" IDENT "(" (param ("," param)*)? ")" "->" type ";"
//!   fn      := "function" IDENT "(" (param ("," param)*)? ")" "->" type block
//!   param   := IDENT ":" type
//!   block   := "{" stmt* "}"
//!   stmt    := "let" "mutable"? IDENT ":" type "=" expr ";"
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
    tokens: Vec<Token>,
    /// Where each token came from, indexed alongside `tokens`. Kept parallel
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
    /// Non-zero while the bounds of a `for i in a..b` are being parsed, and only then.
    /// A range is not a value (see `ast::StmtKind::ForRange` decision 2), so a `..`
    /// reached with this at zero is refused by name rather than by a missing-semicolon
    /// message about a token the author wrote on purpose.
    range_bounds: u32,
    /// The type parameters of the generic being parsed, so `parse_type` can tell `T`
    /// from a struct called `T`. Cleared when the declaration ends.
    type_parameters: Vec<String>,
    /// What `it` stands for, while a contract bracket is being parsed and only then. `Some(name)`
    /// inside `[...]` on `name`'s declaration, `None` everywhere else — which is how `it` manages to
    /// mean the subject in one place and be an ordinary identifier in every other.
    it_means: Option<String>,
    /// Whether THIS clause used `it`, so its message text knows whether to resolve one. Cleared
    /// before every clause.
    used_it: bool,
    /// Whether any bracket on the declaration being parsed used `it`. Needed for the collision rule:
    /// a parameter may still be CALLED `it`, and a bracket that says `it` on such a function is an
    /// error about the collision rather than a silent shadow. Checked once the parameter list is
    /// complete, because a bracket on parameter one cannot know about parameter three.
    ///
    /// Two flags rather than one, and the first attempt at one flag is why: `used_it` is monotonic
    /// if it also has to survive for the collision check, so "did this clause use it" computed as a
    /// CHANGE across the clause answered false for every clause after the first. A return bracket
    /// with two `it` clauses reported the second one unresolved.
    it_seen: bool,
}

impl Parser {
    pub fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self::with_source(tokens, "")
    }

    /// The source is kept so a contract clause can be quoted back exactly as
    /// written when it fails at runtime. Spans alone are not enough: the message is
    /// baked into the compiled program, long after the source has gone.
    pub fn with_source(tokens: Vec<(Token, Span)>, src: &str) -> Self {
        let (tokens, spans) = tokens.into_iter().unzip();
        Parser {
            tokens,
            spans,
            pos: 0,
            allow_struct_lit: true,
            range_bounds: 0,
            src: src.to_string(),
            type_parameters: Vec::new(),
            it_means: None,
            used_it: false,
            it_seen: false,
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
        let mut interfaces = Vec::new();
        let mut impls = Vec::new();
        let mut externs = Vec::new();
        let mut fns = Vec::new();
        let mut methods = Vec::new();
        let mut consts = Vec::new();
        let mut stmts = Vec::new();
        while !self.at(&Token::Eof) {
            if self.at(&Token::Const) {
                consts.push(self.parse_const()?);
            } else if self.at(&Token::Class) {
                let (declared, its_methods, its_associated, its_impls) = self.parse_struct()?;
                structs.push(declared);
                fns.extend(its_associated);
                impls.extend(its_impls);
                // A method declared inside a class is indistinguishable, from here on, from
                // one written outside it. That is the point: nothing downstream changes.
                methods.extend(its_methods);
            } else if self.at(&Token::Enum) {
                enums.push(self.parse_enum()?);
            } else if self.at(&Token::Interface) {
                interfaces.push(self.parse_interface()?);
            } else if self.at(&Token::Impl) {
                impls.push(self.parse_impl()?);
            } else if self.at(&Token::Extern) {
                externs.push(self.parse_extern()?);
            } else if self.at(&Token::Pure) {
                if self.peek_at(1) != &Token::Fn {
                    return Err(format!(
                        "`pure` must be followed by `function`, but found {}",
                        self.peek_at(1).describe()
                    ));
                }
                // `pure function (self: T) name(...)` — a pure METHOD, as of A4. The `(` right
                // after `function` is the same tell that distinguishes a method from a free
                // function below; the only difference here is that `pure` came first.
                //
                // Until v0.0.247 this refused, and the refusal was in the PARSER — which is why
                // `MethodDef` had nowhere to record the marker and why the Rust side needed a new
                // field while the Burxt side needed none. See `MethodDef.is_pure`.
                if self.peek_at(2) == &Token::LParen {
                    methods.push(self.parse_method(None)?);
                } else {
                    fns.push(self.parse_fn()?);
                }
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
        Ok(Program { structs, enums, interfaces, impls, externs, fns, methods, consts, stmts })
    }

    // ---- helpers ----

    /// Peek `offset` tokens ahead without consuming (0 = current).
    fn peek_at(&self, offset: usize) -> &Token {
        self.tokens.get(self.pos + offset).unwrap_or(&Token::Eof)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn at(&self, t: &Token) -> bool {
        self.peek() == t
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
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

    fn parse_struct(&mut self) -> Result<(StructDef, Vec<MethodDef>, Vec<FnDef>, Vec<ImplBlock>), String> {
        let start = self.span().start;
        self.expect(&Token::Class)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected a class name after 'class', found {}", other.describe())),
        };
        let type_parameters = self.parse_type_params(&name)?;
        self.type_parameters = type_parameters.iter().map(|p| p.name.clone()).collect();
        // `class FlatTax implements Tax` — the shape Java, C#, TypeScript and PHP all use, and
        // the reason `interface` had to land first. The class's own methods satisfy it, so this
        // synthesizes an impl block carrying none of its own; the standalone
        // `implement Tax for FlatTax { ... }` stays legal, because it is the only way to add an
        // interface to a class declared somewhere else.
        let mut implements = Vec::new();
        if self.at(&Token::Implements) {
            self.bump();
            loop {
                match self.bump() {
                    // `class Doubler implements Mapper<Int>` — the same instantiation the
                    // standalone `implement Mapper<Int> for Doubler` writes. Both spellings
                    // take arguments or neither does; a generic interface implementable by
                    // one form and not the other would be a rule with a hole in it.
                    Token::Ident(s) => {
                        let mut arguments = Vec::new();
                        if self.at(&Token::Lt) {
                            self.bump();
                            loop {
                                arguments.push(self.parse_type()?);
                                if !self.more_in_list(&Token::Gt) {
                                    break;
                                }
                            }
                            self.expect(&Token::Gt)?;
                        }
                        implements.push((s, arguments));
                    }
                    other => {
                        return Err(format!(
                            "expected an interface name after `implements`, found {}",
                            other.describe()
                        ))
                    }
                }
                if self.at(&Token::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut private_fields = Vec::new();
        let mut associated = Vec::new();
        // Fields, then methods, in one block — which is the whole reason `record` became
        // `class` in v0.0.148. A method here is parsed by `parse_method` with the class as
        // its `owner`, so `function (self) price()` needs no repeated type: a class body is
        // exactly the situation an `implement` block already was. It desugars to the same
        // `MethodDef`, so the typechecker and both backends learn nothing new.
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err(format!("unclosed class `{}`: expected `}}`", name));
            }
            // `private` applies to whichever of the two follows it.
            let is_private = self.at(&Token::Private);
            if is_private {
                self.bump();
            }
            if self.at(&Token::Fn) && self.peek_at(1) != &Token::LParen {
                // No receiver clause: an ASSOCIATED function, which is how a class gets a
                // constructor. `function open(owner: String) -> Account` inside `class Account`
                // is called `Account.open(...)`, and it is the ONLY place a class literal may
                // mention a private field — which is what lets a class defend an invariant.
                //
                // Stored as an ordinary function under the qualified name `Account.open`, so
                // codegen emits `bx.Account.open` exactly as it already does for a method and
                // learns nothing new.
                let mut f = self.parse_fn()?;
                f.name = format!("{}.{}", name, f.name);
                associated.push(f);
                continue;
            }
            if self.at(&Token::Fn) || self.at(&Token::Pure) {
                let mut m = self.parse_method_in_class(&name)?;
                m.private = is_private;
                methods.push(m);
                continue;
            }
            let fname = match self.bump() {
                Token::Ident(s) => s,
                other => {
                    return Err(format!(
                        "expected a field name or a `function` in class {}, found {}",
                        name,
                        other.describe()
                    ))
                }
            };
            self.expect(&Token::Colon)?;
            let ty_start = self.span().start;
            let ty = self.parse_type()?;
            let ty_span = Span { start: ty_start, end: self.prev_end().max(ty_start + 1) };
            // Accepted here only so the typechecker can explain WHY a field
            // cannot have one; "expected `}`" would not teach anything.
            let marshal = self.parse_marshal()?;
            if is_private {
                private_fields.push(fname.clone());
            }
            fields.push(Param { name: fname, ty, marshal, writable: false, ty_span });
            if self.at(&Token::Comma) {
                self.bump(); // trailing comma allowed, and required before a method follows
            }
        }
        self.expect(&Token::RBrace)?;
        self.type_parameters.clear();
        let name_for_impls = name.clone();
        Ok((
            StructDef { name, type_parameters, fields, private_fields, span: Span { start, end: self.prev_end().max(start + 1) } },
            methods,
            associated,
            implements
                .into_iter()
                .map(|(interface_name, interface_arguments)| ImplBlock {
                    interface_name,
                    interface_arguments,
                    type_name: name_for_impls.clone(),
                    methods: Vec::new(),
                    declared_on_class: true,
                    span: Span { start, end: self.prev_end().max(start + 1) },
                })
                .collect(),
        ))
    }

    /// A method written inside a class body.
    ///
    /// `pure` is read here exactly as it is at the top level, because one marker with two
    /// spellings is how a language starts feeling arbitrary. Both routes end in `parse_method`,
    /// which is where the flag is recorded.
    fn parse_method_in_class(&mut self, owner: &str) -> Result<MethodDef, String> {
        self.parse_method(Some(owner))
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
        let type_parameters = self.parse_type_params(&name)?;
        self.type_parameters = type_parameters.iter().map(|p| p.name.clone()).collect();
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
        self.type_parameters.clear();
        Ok(EnumDef { name, type_parameters, variants, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    // ---- interfaces and impls ----

    /// `trait Name { fn m(self) -> T   fn n(mut self, x: U) -> V }`
    /// Signatures only: no bodies, no fields, no default methods.
    fn parse_interface(&mut self) -> Result<InterfaceDef, String> {
        let start = self.span().start;
        self.expect(&Token::Interface)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected an interface name after 'interface', found {}",
                    other.describe()
                ))
            }
        };
        // `interface Mapper<T>` — the same call a generic class and a generic function make,
        // so a bound (`<T: Ordered>`) is spelled and refused identically in all three.
        let type_parameters = self.parse_type_params(&name)?;
        // In scope for every signature in the body, so the `T` of `apply(self, x: T)` parses
        // as `Type::Param` and not as a class nobody declared. Set and cleared exactly where
        // `parse_struct` and `parse_enum` set and clear it.
        self.type_parameters = type_parameters.iter().map(|p| p.name.clone()).collect();
        self.expect(&Token::LBrace)?;
        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err(format!("unclosed interface `{}`: expected `}}`", name));
            }
            methods.push(self.parse_interface_sig(&name)?);
            // B15. This used to skip a `;` here, with the comment "a separating semicolon is
            // allowed but not required" — and that one line was a divergence between the two
            // compilers in the direction that matters: what is ACCEPTED. Stage-1 refuses the
            // `;` form outright, so a program using it compiled with stage-0 and failed to
            // parse with stage-1.
            //
            // Fixed by refusing it here rather than by teaching stage-1 to allow it, because
            // the rest of the language already answers the question: a class body refuses a
            // stray `;` ("expected a field name or a `function`"), and so does an enum. An
            // interface was the ONLY declaration body that took one, which makes it an
            // accident rather than a design — and an optional separator is a second spelling
            // of one thing, which costs every reader the question of which one they are
            // looking at. The same reason closures were declined.
            //
            // Nothing in the suite used the `;` form, which is why the differential could not
            // see this for seventy versions. `tests/fail/interface_signature_semicolon.bx`
            // now writes it down.
        }
        self.expect(&Token::RBrace)?;
        self.type_parameters.clear();
        Ok(InterfaceDef {
            name,
            type_parameters,
            methods,
            span: Span { start, end: self.prev_end().max(start + 1) },
        })
    }

    /// One signature inside an interface: `fn m(self) -> T`, or
    /// `fn m(mut self, extra: U) -> V`. The receiver has no type — it is
    /// whichever type implements the interface.
    fn parse_interface_sig(&mut self, interface_name: &str) -> Result<InterfaceSig, String> {
        // Stage-1's wording, adopted here rather than left to differ. B15 removed the trailing
        // `;` stage-0 used to accept, which made this the message a reader now meets — and the
        // two compilers said it differently: stage-0's bare `expected `function`, found `;`` and
        // stage-1's `expected `function` — an interface holds signatures, found `;``. The second
        // says WHY, so it is the one that survives. Two compilers agreeing on a refusal is the
        // point of having two, and B19 already cost a version by fixing that in the other order.
        if !self.at(&Token::Fn) {
            return Err(format!(
                "expected `function` — an interface holds signatures, found {}",
                self.peek().describe()
            ));
        }
        self.expect(&Token::Fn)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a method name in interface `{}`, found {}",
                    interface_name,
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
                "every interface method takes `self` first: write `function {}({}self, ...)`, \
                 found {}",
                name,
                if receiver_mut { "mutable " } else { "" },
                self.peek().describe()
            ));
        }
        self.bump();
        let mut parameters = Vec::new();
        while self.at(&Token::Comma) {
            self.bump();
            let writable = if self.at(&Token::Mut) {
                self.bump();
                true
            } else {
                false
            };
            let pname = match self.bump() {
                Token::Ident(s) => s,
                other => {
                    return Err(format!("expected a parameter name, found {}", other.describe()))
                }
            };
            self.expect(&Token::Colon)?;
            let ty_start = self.span().start;
            let ty = self.parse_type()?;
            let ty_span = Span { start: ty_start, end: self.prev_end().max(ty_start + 1) };
            parameters.push(Param { name: pname, ty, marshal: None, writable, ty_span });
        }
        self.expect(&Token::RParen)?;
        if !self.at(&Token::Arrow) {
            return Err(format!(
                "expected `->` and a return type for `{}.{}` (every Burxt function \
                 returns a value), found {}",
                interface_name,
                name,
                self.peek().describe()
            ));
        }
        self.bump();
        let ret = self.parse_type()?;
        Ok(InterfaceSig { name, receiver_mut, parameters, ret })
    }

    /// `impl Trait for Type { <method definitions> }`
    fn parse_impl(&mut self) -> Result<ImplBlock, String> {
        let start = self.span().start;
        self.expect(&Token::Impl)?;
        let interface_name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected an interface name after 'implement', found {}",
                    other.describe()
                ))
            }
        };
        // `implement Mapper<Int> for Doubler` — implementing a generic interface AT a
        // concrete instantiation. The arguments travel beside the name rather than mangled
        // into it, because the parser does not know which names are generic and a message
        // has to be able to say `Mapper<Int>`, which is what the author wrote.
        let mut interface_arguments = Vec::new();
        if self.at(&Token::Lt) {
            self.bump();
            loop {
                interface_arguments.push(self.parse_type()?);
                if !self.more_in_list(&Token::Gt) {
                    break;
                }
            }
            self.expect(&Token::Gt)?;
        }
        if !self.at(&Token::For) {
            return Err(format!(
                "expected `for` in `implement {} for Type`, found {}",
                interface_name,
                self.peek().describe()
            ));
        }
        self.bump();
        let type_name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a type name in `implement {} for ...`, found {}",
                    interface_name,
                    other.describe()
                ))
            }
        };
        self.expect(&Token::LBrace)?;
        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Eof) {
                return Err(format!(
                    "unclosed `implement {} for {}`: expected `}}`",
                    interface_name, type_name
                ));
            }
            methods.push(self.parse_method(Some(&type_name))?);
        }
        self.expect(&Token::RBrace)?;
        Ok(ImplBlock { interface_name, interface_arguments, type_name, methods, declared_on_class: false, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    // ---- functions ----

    fn parse_extern(&mut self) -> Result<ExternFn, String> {
        let start = self.span().start;
        self.expect(&Token::Extern)?;
        self.expect(&Token::Fn)?;
        let (name, type_parameters, parameters, ret, bracket_requires, bracket_ensures) =
            self.parse_fn_signature()?;
        // An `external function` is a declaration, not a definition: there is no body to insert a
        // check into, and the C function on the other side will not honour a promise it cannot see.
        // Refused by name rather than accepted and ignored.
        if !bracket_requires.is_empty() || !bracket_ensures.is_empty() {
            return Err(format!(
                "`external function {}` cannot carry a contract: there is no body to check it in, \
                 and C will not honour a promise it cannot see. Check the values before you call \
                 it, in a Burxt function that can.",
                name
            ));
        }
        if !type_parameters.is_empty() {
            // C has no notion of a type parameter, and a monomorphised C symbol is a
            // symbol that does not exist. See spec/M7-GENERICS.md §2.
            return Err(format!(
                "`external function {}` cannot be generic: C has no type parameters, and there \
                 would be no symbol to link against.",
                name
            ));
        }
        // The one place `touches` is REQUIRED reasoning rather than optional: there is no body
        // here, so whatever this C function reaches, only this line can say. An extern that
        // declares nothing is taken at its word — it touches nothing — which is right for
        // `strlen` and a lie for `system`, so the standard library declares its own.
        let touches = self.parse_touches()?;
        self.expect(&Token::Semicolon)?;
        Ok(ExternFn { name, parameters, ret, touches, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    /// Contract clauses sit between the signature and the body, where a reader
    /// looks for what a function demands and promises. Shared by functions and
    /// methods, so the two can never drift.
    /// `[> $0.00, < balance]` after a type — contracts on the value that type describes.
    ///
    /// The subject is ELIDED: a clause beginning with a comparison operator gets `subject` inserted
    /// on its left, so `[> $0.00]` on a parameter named `amount` becomes `amount > $0.00`. Position
    /// decides what the clause is about, which is why there is no `self` here to be confused with a
    /// method's receiver. See spec/M13-CONTRACT-SYNTAX.md Decision 1.
    ///
    /// A clause that needs the subject anywhere else writes `it` (spec Decision 2), resolved at the
    /// one place a bare identifier becomes a `Var` — see `it_means`. Shipped in v0.0.167, after this
    /// comment spent thirty-two versions claiming it was "resolved later against a binding this
    /// function installs nowhere. The checker is what knows it means the subject." The checker did
    /// not, nothing did, and `[it * 2 > 0]` answered `unknown variable: it` the whole time.
    ///
    /// Worth leaving the history here: a comment describing behaviour that does not exist reads
    /// exactly like one describing behaviour that does.
    ///
    /// Each comma is a SEPARATE clause rather than one `&&`, so a failure can name the one that
    /// broke. Decision 3.
    fn parse_value_contracts(&mut self, subject: &str) -> Result<Vec<Contract>, String> {
        let mut clauses = Vec::new();
        if !self.at(&Token::LBracket) {
            return Ok(clauses);
        }
        self.bump();
        if self.at(&Token::RBracket) {
            return Err(format!(
                "`[]` after the type of `{}` promises nothing. Write a clause — `[> 0]` — or \
                 leave the brackets off.",
                subject
            ));
        }
        loop {
            let start = self.span().start;
            self.used_it = false;
            let leading = match self.peek() {
                Token::Gt => Some(CmpOp::Gt),
                Token::Lt => Some(CmpOp::Lt),
                Token::Ge => Some(CmpOp::Ge),
                Token::Le => Some(CmpOp::Le),
                Token::EqEq => Some(CmpOp::Eq),
                Token::NotEq => Some(CmpOp::Ne),
                _ => None,
            };
            let cond = if let Some(op) = leading {
                // The elided form. The subject is synthesized at the operator's own span, so a
                // failure points at the clause the reader wrote rather than at the parameter.
                self.bump();
                let here = Span { start, end: self.prev_end().max(start + 1) };
                let lhs = Expr { kind: ExprKind::Var(subject.to_string()), span: here };
                let rhs = self.parse_cond()?;
                Expr {
                    kind: ExprKind::Compare { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                    span: Span { start, end: self.prev_end().max(start + 1) },
                }
            } else {
                // `it` means the subject for exactly the length of this clause. Set and restored
                // rather than left on, because `it` is an ordinary name everywhere else — and a
                // capability left switched on is the bug shape stage-1's `current_receiver` had.
                let outer = self.it_means.take();
                self.it_means = Some(subject.to_string());
                let parsed = self.parse_cond();
                self.it_means = outer;
                parsed?
            };
            // Per CLAUSE, not per signature: `used_it` only ever goes from false to true, and it has
            // to stay true for the collision check below, so "did THIS clause use it" is the change
            // across it rather than its value.
            let clause_used_it = self.used_it;
            if clause_used_it {
                self.it_seen = true;
            }
            let span = Span { start, end: self.prev_end().max(start + 1) };
            // The text classes the clause as a reader would WRITE it, subject included — so an
            // elided `[<= balance]` on `amount` reports `amount <= balance` rather than a fragment
            // that does not say which value broke.
            //
            // It also makes the desugaring observable: the same program written with brackets and
            // with `requires` produces byte-identical failure messages, which is what
            // `bracket_contracts_desugar_to_the_same_message` in tests/runner.rs checks rather than
            // asserts — it compiles both spellings and compares the stderr.
            //
            // This comment used to cite `tests/pass/contract_brackets.bx`, which never existed. The
            // bracket form went fourteen versions with no fixture anywhere, cited by a comment that
            // read as though it had one.
            let written = self.text_of(span);
            let text = if leading.is_some() {
                format!("{} {}", subject, written)
            } else if clause_used_it {
                // `it` is resolved in the MESSAGE too, not only in the condition. Reporting
                // `` `requires it > $0.00` `` would name no value, which is precisely the tax the
                // synthesized-subject decision was taken to avoid: the reader would have to go back
                // to the declaration to learn what `it` was. So `[it > $0.00 || it < $10.00]` on
                // `balance` reports `balance > $0.00 || balance < $10.00`.
                //
                // A whole-word replacement over the written text, which is the honest instrument
                // here: the alternative is a pretty-printer for contract expressions, and its output
                // would then differ from the source spelling for every OTHER clause in the language.
                // The one thing it gets wrong is a bare `it` inside a string literal in a bracket
                // clause, which is why the check is whole-word rather than a substring.
                replace_whole_word(&written, "it", subject)
            } else {
                written
            };
            clauses.push(Contract { cond, text, span });
            if !self.more_in_list(&Token::RBracket) {
                break;
            }
        }
        self.expect(&Token::RBracket)?;
        Ok(clauses)
    }

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
        let (name, type_parameters, parameters, ret, bracket_requires, bracket_ensures) =
            self.parse_fn_signature()?;
        // `-> T allocates` reads as what it is: returns a T, and allocates. `-> T allocates nothing`
        // is the opposite CLAIM, and the compiler holds the signature to it.
        let mut allocates = self.at_word("allocates");
        let mut allocates_nothing = false;
        if allocates {
            self.bump();
            if self.at_word("nothing") {
                self.bump();
                allocates = false;
                allocates_nothing = true;
            }
        }
        // After `allocates`, before the clauses: a signature reads left to right as what it
        // answers, what it builds, what it reaches, and what it promises.
        let touches = self.parse_touches()?;
        let (mut requires, mut ensures, decimal_decreases) = self.parse_contracts()?;
        let decreases = decimal_decreases;
        // Brackets FIRST, then the written clauses. A precondition on a parameter is about a value
        // the caller already passed, so it should be the first thing checked and the first thing
        // reported — and the escape-hatch `requires`, which is usually about the call as a whole,
        // reads naturally after.
        let mut all_requires = bracket_requires;
        all_requires.append(&mut requires);
        let requires = all_requires;
        let mut all_ensures = bracket_ensures;
        all_ensures.append(&mut ensures);
        let ensures = all_ensures;
        let body = self.parse_block()?;
        // The parameters are only in scope for this signature and body.
        self.type_parameters.clear();
        Ok(FnDef { name, type_parameters, parameters, ret, allocates, allocates_nothing, touches, is_pure, requires, ensures, decreases, body, span: Span { start, end: self.prev_end().max(start + 1) } })
    }

    /// `fn (self: Type) name(parameters) -> ret { body }`, or `fn (mut self: ...)`
    /// for a mutating method.
    fn parse_method(&mut self, owner: Option<&str>) -> Result<MethodDef, String> {
        // `pure`, read in the ONE place a method is built, so there is one spelling of the rule
        // and one place the flag comes from. Every route leaves the token in place for this —
        // the top level, a class body and an `implement` block alike.
        let is_pure = self.at(&Token::Pure);
        if is_pure {
            self.bump();
        }
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
                "expected `self` in the receiver clause `function ({}self: Type)`, found {}",
                if receiver_mut { "mutable " } else { "" },
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
                        "`function (self)` needs the type: outside an `implement` block nothing says \
                         which one. Write `function (self: Type) name(...)`, or put the method \
                         in `implement Trait for Type { ... }`, where the header says it once."
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
                        "expected a class name after `self:`, found {}",
                        other.describe()
                    ))
                }
            }
        };
        // `self: Stack<T>` — the receiver names the class's own type parameters, which are
        // then in scope for the rest of the signature and the body.
        let mut receiver_arguments: Vec<String> = Vec::new();
        if self.at(&Token::Lt) {
            self.bump();
            loop {
                match self.bump() {
                    Token::Ident(a) => receiver_arguments.push(a),
                    other => {
                        return Err(format!(
                            "`self: {}<...>` names the class's type parameters, so each one \
                             must be a name; found {}",
                            receiver,
                            other.describe()
                        ))
                    }
                }
                if !self.more_in_list(&Token::Gt) {
                    break;
                }
            }
            self.expect(&Token::Gt)?;
        }
        self.type_parameters = receiver_arguments.clone();
        self.expect(&Token::RParen)?;
        let (name, type_parameters, parameters, ret, bracket_requires, bracket_ensures) =
            self.parse_fn_signature()?;
        if !type_parameters.is_empty() {
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
        // After `allocates`, before the clauses: a signature reads left to right as what it
        // answers, what it builds, what it reaches, and what it promises.
        let touches = self.parse_touches()?;
        let (mut requires, mut ensures, decimal_decreases) = self.parse_contracts()?;
        let decreases = decimal_decreases;
        // Same order as a free function: brackets first, then the written clauses.
        let mut all_requires = bracket_requires;
        all_requires.append(&mut requires);
        let requires = all_requires;
        let mut all_ensures = bracket_ensures;
        all_ensures.append(&mut ensures);
        let ensures = all_ensures;
        if let Some(d) = &decreases {
            let _ = d;
            return Err(
                "`decreases` on a method is not built yet — the measure goes on a free \
                 function for now."
                    .to_string(),
            );
        }
        let body = self.parse_block()?;
        self.type_parameters.clear();
        Ok(MethodDef { receiver, private: false, is_pure, touches, receiver_arguments, receiver_mut, name, parameters, ret, allocates, requires, ensures, body, span: Span { start, end: self.prev_end().max(start + 1) } })
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

    fn parse_type_params(&mut self, owner: &str) -> Result<Vec<TypeParam>, String> {
        if !self.at(&Token::Lt) {
            return Ok(Vec::new());
        }
        self.bump();
        let mut names: Vec<TypeParam> = Vec::new();
        loop {
            match self.bump() {
                Token::Ident(s) => {
                    if names.iter().any(|p| p.name == s) {
                        return Err(format!(
                            "`{}` declares the type parameter `{}` twice",
                            owner, s
                        ));
                    }
                    // `<T: Ordered>` — one bound per parameter. Two would need a `where`
                    // clause to stay readable, and one has covered every case so far.
                    let bound = if self.at(&Token::Colon) {
                        self.bump();
                        match self.bump() {
                            Token::Ident(b) => Some(b),
                            other => {
                                return Err(format!(
                                    "expected an interface name after `{}:`, found {} — a \
                                     bound is `Ordered`, `Equatable`, or an interface this \
                                     program declares.",
                                    s,
                                    other.describe()
                                ))
                            }
                        }
                    } else {
                        None
                    };
                    names.push(TypeParam { name: s, bound });
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

    /// Also answers the contracts written as brackets on the parameters and the return type,
    /// already desugared to ordinary `requires`/`ensures` clauses. The caller appends them to
    /// whatever `parse_contracts` finds, so the two forms are the same thing by the time anything
    /// downstream sees them — which is why this milestone touches almost nothing but the parser.
    fn parse_fn_signature(
        &mut self,
    ) -> Result<(String, Vec<TypeParam>, Vec<Param>, Type, Vec<Contract>, Vec<Contract>), String> {
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected a function name after 'function', found {}", other.describe())),
        };
        // `fn name<T, U>(...)`. Recorded on the parser so `parse_type` can tell a type
        // parameter from a struct name — which is the only place that distinction is
        // visible, since both are spelled as a bare identifier.
        let type_parameters = self.parse_type_params(&name)?;
        // EXTEND, not replace: a method's receiver has already put the class's parameters
        // in scope (`self: Stack<T>`), and replacing them turned `item: T` into an ordinary
        // NAMED type — which then looked identical when printed and silently failed to
        // substitute at instantiation. Cleared after each declaration, so nothing leaks.
        self.type_parameters.extend(type_parameters.iter().map(|p| p.name.clone()));
        self.expect(&Token::LParen)?;
        let mut parameters = Vec::new();
        let mut bracket_requires: Vec<Contract> = Vec::new();
        self.it_seen = false;
        if !self.at(&Token::RParen) {
            loop {
                // `mutable xs: [Int]` — the callee may modify the caller's value. Read here rather
                // than as part of the type, because it is a fact about the CROSSING and not about
                // what the value is: two parameters of the same type can differ in it.
                let writable = if self.at(&Token::Mut) {
                    self.bump();
                    true
                } else {
                    false
                };
                let pname = match self.bump() {
                    Token::Ident(s) => s,
                    other => return Err(format!("expected a parameter name, found {}", other.describe())),
                };
                self.expect(&Token::Colon)?;
                let ty_start = self.span().start;
                let ty = self.parse_type()?;
                let ty_span = Span { start: ty_start, end: self.prev_end().max(ty_start + 1) };
                let marshal = self.parse_marshal()?;
                // Brackets AFTER the marshaller, so `as scaled` still reads as part of the type
                // and the contract reads as a statement about the value.
                bracket_requires.extend(self.parse_value_contracts(&pname)?);
                parameters.push(Param { name: pname, ty, marshal, writable, ty_span });
                if !self.more_in_list(&Token::RParen) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen)?;
        // The collision rule, checked here because a bracket on parameter one cannot know about a
        // parameter three called `it`. `it` is NOT a keyword — a program may still name something
        // `it` — but a function that has such a parameter AND a bracket saying `it` has two meanings
        // for one word, and picking either would be a silent shadow. The same rule `result` follows
        // inside `ensures`, which is the point: one decision, applied twice, rather than a second
        // mechanism to remember.
        if self.it_seen && parameters.iter().any(|p| p.name == "it") {
            return Err(format!(
                "`{}` has a parameter called `it`, and a contract bracket that says `it` — so `it` \
                 would mean two things in one signature. Inside a bracket `it` is the value the \
                 bracket is about; rename the parameter, or write its name in the clause instead \
                 of `it`.",
                name
            ));
        }
        if !self.at(&Token::Arrow) {
            return Err(format!(
                "expected `->` and a return type after function {}'s parameter list \
                 (every Burxt function returns a value), found {}",
                name,
                self.peek().describe()
            ));
        }
        self.bump();
        let ret = self.parse_type()?;
        // On the return type the subject is `result` — the same name `ensures` already binds, so a
        // bracket there is an `ensures` in every respect. It is also the "on exit" slot, which is
        // where a conservation law lives even though it is not about the returned value.
        let bracket_ensures = self.parse_value_contracts("result")?;
        Ok((name, type_parameters, parameters, ret, bracket_requires, bracket_ensures))
    }

    /// `touches files, commands` — what this function reaches outside itself.
    ///
    /// Contextual, like `allocates` and `requires`: recognised by position rather than reserved,
    /// so a program may still name a variable `touches`. The effect names are ordinary
    /// identifiers checked against a closed vocabulary, so a typo is an error naming the whole
    /// list rather than a silently invented effect.
    fn parse_touches(&mut self) -> Result<Vec<Effect>, String> {
        let mut effects = Vec::new();
        if !self.at_word("touches") {
            return Ok(effects);
        }
        self.bump();
        loop {
            let word = match self.bump() {
                Token::Ident(s) => s,
                other => {
                    return Err(format!(
                        "expected an effect name after `touches`, found {}. The effects are: {}.",
                        other.describe(),
                        Effect::all()
                    ))
                }
            };
            match Effect::parse(&word) {
                Some(e) => {
                    if effects.contains(&e) {
                        return Err(format!("`touches` lists `{}` twice", e));
                    }
                    effects.push(e);
                }
                None => {
                    return Err(format!(
                        "`{}` is not an effect this language knows. The effects are: {}.",
                        word,
                        Effect::all()
                    ))
                }
            }
            if self.at(&Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        effects.sort();
        Ok(effects)
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
            Token::Print => self.parse_print(false),
            Token::PrintError => self.parse_print(true),
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
            // A `const` inside a block, answered by name rather than by "expected statement".
            //
            // Top-level only, and the reason is that a function-local `const` would be a
            // second spelling for something the language already has. `let` without
            // `mutable` is immutable, it is folded nowhere and needs to be folded nowhere
            // — the value is one line above the use — and its scope is exactly the block
            // it is written in, which is what a local wants. The whole delta of `const`
            // over `let` is program-wide scope plus literal substitution, and neither
            // means anything inside one body.
            //
            // Cost, stated rather than hidden: a helper that wants a private magic number
            // has to put it at the top of the file, where every other function can see it
            // too. That is a real loss of locality, and the alternative — two constructs
            // with the same semantics and different spellings — is worse for the reviewer
            // this language is for.
            Token::Const => Err("a `const` goes at the top level, not inside a block: it is \
                                 a name for the whole program, and it is in scope inside \
                                 every function. Move it above, or use `let` — which is \
                                 already immutable without `mutable`."
                .to_string()),
            other => Err(format!("expected statement, found {}", other.describe())),
        }
    }

    /// A statement starting with an identifier (or `self`) is one of:
    ///   name = value;                  assignment
    ///   name[index] = value;           element assignment
    ///   name.a.b = value;              field assignment
    ///   name(arguments);                    a call kept for its side effect
    ///   name.a.b.method(arguments);         a method call kept for its side effect
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
            let arguments = self.parse_call_args()?;
            self.expect(&Token::Semicolon)?;
            return Ok(StmtKind::ExprStmt(self.expr(ExprKind::Call { name, arguments }, start)));
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
                // `p.0 = 7;` — a tuple position on the left of an assignment. The same
                // spelling the READ side accepts, and it has to be accepted here or the
                // language would let you read a position and not write one, for no reason
                // a reader could name. It costs one arm because `AssignField` already
                // carries the path as strings and resolves it through `resolve_field`,
                // which is where "0" is already a field name.
                Token::Int(n) if n >= 0 => n.to_string(),
                other => return Err(format!("expected a field name after '.', found {}", other.describe())),
            };
            if self.at(&Token::LParen) {
                let arguments = self.parse_call_args()?;
                self.expect(&Token::Semicolon)?;
                let mut base = self.expr(ExprKind::Var(name), start);
                for f in path {
                    base = self.expr(ExprKind::Field { base: Box::new(base), field: f }, start);
                }
                return Ok(StmtKind::ExprStmt(
                    self.expr(ExprKind::MethodCall { base: Box::new(base), method: seg, arguments }, start),
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
        let mut arguments = Vec::new();
        if !self.at(&Token::RParen) {
            loop {
                arguments.push(self.parse_expr()?);
                if !self.more_in_list(&Token::RParen) {
                    break;
                }
            }
        }
        self.expect(&Token::RParen)?;
        Ok(arguments)
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
            // A pattern is a variant name, or a LITERAL when the subject is a scalar. `_` lexes
            // as an identifier, so the wildcard arrives through the Ident arm.
            let mut literal = None;
            let variant = match self.bump() {
                Token::Ident(s) => s,
                Token::Int(n) => {
                    literal = Some(MatchLiteral::Int(n));
                    n.to_string()
                }
                Token::Str(s) => {
                    literal = Some(MatchLiteral::Text(s.clone()));
                    format!("\"{}\"", s)
                }
                Token::True => {
                    literal = Some(MatchLiteral::Truth(true));
                    "true".to_string()
                }
                Token::False => {
                    literal = Some(MatchLiteral::Truth(false));
                    "false".to_string()
                }
                other => {
                    return Err(format!(
                        "expected a variant name or a literal to match on, found {}",
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
            arms.push(MatchArm { variant, bindings, body, literal });
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

    /// `for x in xs { body }` and `for i in 0..n { body }`. See spec/M10-ERGONOMICS.md §1b
    /// for the array form and `ast::StmtKind::ForRange` for the range form's five decisions.
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
        // `..` is legal only between the two bounds of a `for`, so the flag is raised
        // only here and the check in `parse_expr` refuses it everywhere else BY NAME.
        // A counter rather than a bool because a `for` can nest inside a bound's
        // parentheses in principle, and a bool would be cleared by the inner one.
        self.range_bounds += 1;
        let first = self.parse_cond();
        self.range_bounds -= 1;
        let first = first?;
        if self.at(&Token::DotDot) {
            self.bump();
            self.range_bounds += 1;
            let end = self.parse_cond();
            self.range_bounds -= 1;
            let end = end?;
            // `0..1..2`. Caught here rather than left to "expected `{`, found `..`",
            // because the author plainly meant a range and the answer is that a range
            // has two bounds, not that a brace is missing.
            if self.at(&Token::DotDot) {
                return Err(
                    "a range has exactly two bounds — `0..n`. There is no `a..b..c`, and \
                     no step: a loop that skips writes the skip in its body."
                        .to_string(),
                );
            }
            let body = self.parse_block()?;
            return Ok(StmtKind::ForRange { name, start: first, end, body });
        }
        let iterable = first;
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

    /// `const NAME: Type = value;` — an ITEM, so it is parsed here beside the other
    /// items rather than in `parse_stmt_kind`.
    ///
    /// The grammar is a `let` minus `mutable`, with the annotation REQUIRED rather than
    /// optional: neither of those choices would mean anything on a name the whole program
    /// shares. Nothing is evaluated here — the initializer is kept as an ordinary `Expr`
    /// and folded by the typechecker, which is where a fold that OVERFLOWS has to be
    /// reported.
    ///
    /// It could have been folded here instead, and that was the first design: the
    /// parser knows the consts declared above and could substitute a literal on the
    /// spot, leaving the typechecker and codegen untouched in BOTH compilers. It was
    /// dropped for one reason, and it is a testing reason rather than a taste one. A
    /// refusal reported by the parser is a parse error, and stage-1's agreement with
    /// stage-0 is measured by counting `type errors:` — so every `const` refusal would
    /// have been invisible to the ratchet, exactly as the five `bracket_*` fixtures were
    /// for two versions when stage-1 could not parse a bracket at all. Folding in the
    /// checker costs a name-resolution fallback in each compiler and buys a refusal both
    /// of them are measured on.
    fn parse_const(&mut self) -> Result<ConstDef, String> {
        let start = self.span().start;
        self.expect(&Token::Const)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a name after `const`, found {}",
                    other.describe()
                ))
            }
        };
        // Required, unlike `let`. See `ast::ConstDef::declared` for why.
        if !self.at(&Token::Colon) {
            return Err(format!(
                "`const {}` needs its type written out, as in `const {}: Int = 1;`. \
                 A `const` is a name every function in the program can read, and this \
                 line is the only place a reader will look to find out what it is — so \
                 unlike `let`, the annotation is not optional.",
                name, name
            ));
        }
        self.bump();
        let declared = self.parse_type()?;
        self.expect(&Token::Equals)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(ConstDef {
            name,
            declared,
            value,
            span: Span { start, end: self.prev_end().max(start + 1) },
        })
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

    /// `print(x);` and `print_error(x);` — the same statement with a different destination.
    fn parse_print(&mut self, to_stderr: bool) -> Result<StmtKind, String> {
        self.expect(if to_stderr { &Token::PrintError } else { &Token::Print })?;
        self.expect(&Token::LParen)?;
        let e = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Semicolon)?;
        Ok(StmtKind::Print { value: e, to_stderr })
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
            self.tokens[self.pos] = Token::Equals;
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
            // One arm, because the lexer already carried the two numbers. Whether a width is
            // ALLOWED here is `validate_type`'s job, not the parser's — the parser's business is
            // what was written, and refusing it at the boundary check is what makes the message say
            // where a width may go rather than "unexpected token".
            Token::TyWidth { bits, signed } => Ok(Type::Width { bits, signed }),
            Token::TyCPointer => Ok(Type::CPointer),
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
                if self.type_parameters.contains(&name) {
                    return Ok(Type::Param(name));
                }
                // `Option<Int>` — a generic applied. `<` after a type name can only mean
                // this, so no lookahead beyond the one token is needed.
                if self.at(&Token::Lt) {
                    self.bump();
                    let mut arguments = Vec::new();
                    loop {
                        arguments.push(self.parse_type()?);
                        if !self.more_in_list(&Token::Gt) {
                            break;
                        }
                    }
                    self.expect(&Token::Gt)?;
                    return Ok(Type::Generic { name, arguments });
                }
                Ok(Type::Named(name))
            }
            // `dyn Trait` — the only syntax that asks for dynamic dispatch.
            // If you never write `dyn`, you never pay for a vtable.
            Token::Dyn => match self.bump() {
                Token::Ident(name) => {
                    // `dynamic Mapper<Int>` — an interface object at an instantiation, A9.
                    // Kept distinct from a bare `Mapper<Int>` until `expand`; see
                    // `ast::Type::DynGeneric` for the program that measures why.
                    if self.at(&Token::Lt) {
                        self.bump();
                        let mut arguments = Vec::new();
                        loop {
                            arguments.push(self.parse_type()?);
                            if !self.more_in_list(&Token::Gt) {
                                break;
                            }
                        }
                        self.expect(&Token::Gt)?;
                        return Ok(Type::DynGeneric { name, arguments });
                    }
                    Ok(Type::Dyn(name))
                }
                other => Err(format!(
                    "expected an interface name after `dynamic`, found {}",
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
            // `(Int, String)` — a tuple. Arity two or more, and both ends of that are
            // refusals rather than silences: `()` is a type with no values, which Burxt has
            // no use for and no way to build, and `(Int)` is a reader writing parentheses
            // around a type. Rust answers the second with `(Int,)`, a trailing comma that
            // changes the type — exactly the one-character difference `ForRange` refuses
            // `..=` for. So a one-element tuple is unspellable here, on purpose.
            Token::LParen => {
                if self.at(&Token::RParen) {
                    return Err(
                        "`()` is not a type: a tuple holds two or more values, and Burxt \
                         has no unit type for the empty one to be."
                            .to_string(),
                    );
                }
                let mut elements = vec![self.parse_type()?];
                while self.at(&Token::Comma) {
                    self.bump();
                    elements.push(self.parse_type()?);
                }
                // Raised BEFORE the `)` is eaten, so the caret lands on the `)` that closed the
                // thing being complained about rather than on whatever followed it. Stage-1
                // points at the same token, and a span the two compilers agree on is one fewer
                // row in the corpus's span column (§B17).
                if elements.len() < 2 {
                    return Err(format!(
                        "a tuple holds two or more values, so `({})` is just `{}` — drop the \
                         parentheses. There is no one-value tuple in Burxt: it would differ \
                         from a parenthesised type by one comma.",
                        elements[0], elements[0]
                    ));
                }
                self.expect(&Token::RParen)?;
                Ok(Type::Tuple(elements))
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
        // A range is a `for` construct, never a value — one place in the grammar, and this
        // is the check that keeps it there. Placed at the TOP of the expression chain
        // rather than in `parse_primary`, because `..` follows a complete expression and
        // the only way to see it is to have finished one. See `ast::StmtKind::ForRange`
        // decision 2 for why waiting for A11 beats inventing half an iterator now.
        if self.range_bounds == 0 && self.at(&Token::DotDot) {
            return Err(
                "`..` builds a range, and a range is only a `for` construct — there is no \
                 range VALUE yet. Write `for i in 0..n { ... }`. A range that can be \
                 stored, passed and chained needs an iterator protocol, which is a \
                 separate piece of work; half of one would be worse than waiting."
                    .to_string(),
            );
        }
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
    /// `.method(arguments)` calls, optionally negated:
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
            // `e?` binds tighter than any operator, like a field read — `f(x)? + 1` is
            // "the value, or return the failure" and then plus one.
            if self.at(&Token::Question) {
                self.bump();
                e = self.expr(ExprKind::Try(Box::new(e)), start);
                continue;
            }
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
                // `pair.0` — a tuple's field name IS its position, and the lexer already
                // produced it: `lex_number` claims a `.` only when a DIGIT follows, so
                // `x.0` was Ident/Dot/Int before A8 existed and neither lexer changed.
                //
                // The position is carried as the STRING "0", which is what makes this
                // reuse `ExprKind::Field` rather than add a kind: a declared field name
                // comes from an `Ident`, so it can never be a run of digits, and the two
                // can never collide in `fields_of`.
                Token::Int(n) if n >= 0 => n.to_string(),
                // `t.0.1` does NOT lex the way it reads, and this is the arm that says so
                // instead of letting the reader work it out. After the first `.`,
                // `lex_number` sees `0`, then `.`, then the digit `1`, and claims all three
                // as the decimal `0.1` — the same lookahead that makes `x.0` work at one
                // level makes it impossible at two. Found by writing the program, not by
                // reading the lexer.
                Token::Decimal(unscaled, scale) if scale > 0 => {
                    let ten: i64 = 10i64.pow(scale);
                    return Err(format!(
                        "`.{}.{}` cannot be written: after a `.`, `{}.{}` is lexed as one \
                         decimal literal rather than two positions. Bind the inner tuple \
                         first — `let inner = t.{}; inner.{}`.",
                        unscaled / ten,
                        unscaled % ten,
                        unscaled / ten,
                        unscaled % ten,
                        unscaled / ten,
                        unscaled % ten
                    ));
                }
                other => return Err(format!("expected a field or method name after '.', found {}", other.describe())),
            };
            if self.at(&Token::LParen) {
                self.bump();
                let mut arguments = Vec::new();
                if !self.at(&Token::RParen) {
                    loop {
                        arguments.push(self.parse_expr()?);
                        if !self.more_in_list(&Token::RParen) {
                            break;
                        }
                    }
                }
                self.expect(&Token::RParen)?;
                e = self.expr(ExprKind::MethodCall { base: Box::new(e), method: name, arguments }, start);
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
            // A tuple literal and a parenthesised expression begin identically and are told
            // apart by the comma, which is why this lives here rather than in a case of its
            // own: `(a + b)` must keep costing nothing and keep ITS OWN span.
            let e = if e.is_ok() && self.at(&Token::Comma) {
                let mut elements = vec![e?];
                let mut trailing = false;
                while self.at(&Token::Comma) {
                    self.bump();
                    if self.at(&Token::RParen) {
                        trailing = true;
                        break;
                    }
                    elements.push(self.parse_expr()?);
                }
                // Every other comma-separated list in Burxt tolerates a trailing comma
                // (`more_in_list`). A tuple must not, and this is the one place the
                // difference is worth the extra branch: `(a,)` is how Rust spells a
                // ONE-value tuple, so accepting it here would silently build a two-value
                // one from a reader who meant something else.
                if trailing {
                    self.allow_struct_lit = saved;
                    return Err(
                        "a tuple literal takes no trailing comma. `(a,)` is a one-value \
                         tuple in some other languages and Burxt has none — write `(a, b)` \
                         for the pair, or drop the parentheses for the value."
                            .to_string(),
                    );
                }
                Ok(self.expr(ExprKind::TupleLit(elements), start))
            } else {
                e
            };
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
                            let tokens = crate::lexer::Lexer::new(&src).tokenize().map_err(|e| {
                                format!("in the interpolation `{{{}}}`: {}", src.trim(), e.message)
                            })?;
                            let mut sub = Parser::new(tokens);
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
                    let mut arguments = Vec::new();
                    if !self.at(&Token::RParen) {
                        loop {
                            arguments.push(self.parse_expr()?);
                            if !self.more_in_list(&Token::RParen) {
                                break;
                            }
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(ExprKind::Call { name: s, arguments })
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
                } else if s == "it" && self.it_means.is_some() {
                    // Inside a contract bracket, `it` IS the subject — spec M13 Decision 2. Resolved
                    // here, at the one place a bare name becomes a `Var`, rather than by walking the
                    // parsed expression afterwards: a walker would have to know every variant that
                    // can hold an expression, and forgetting one is a silent miss.
                    //
                    // Everywhere else `it` is an ordinary identifier, which is what `it_means` being
                    // None encodes. Not a keyword: a program may still call something `it`.
                    self.used_it = true;
                    Ok(ExprKind::Var(self.it_means.clone().unwrap()))
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

/// Replace every whole-word occurrence of `word` with `with`.
///
/// Whole-word so that `it` inside `limit`, `omit` or `items` is left alone — a substring replace here
/// would rewrite a clause into nonsense, silently, in a runtime message nobody looks at until it
/// fires. A word boundary is "not alphanumeric and not `_`", the same set the lexer uses to end an
/// identifier, so the two agree about where a name stops.
fn replace_whole_word(text: &str, word: &str, with: &str) -> String {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    // B8. Inside a string literal, `it` is TEXT and not the subject. Without this, a clause
    // written `[it != "make it so"]` reported `requires tag != "make tag so"` — a message quoting
    // a string the program does not contain. Cosmetic in the sense that nothing computes wrongly,
    // and not cosmetic at all in a language whose case is that a reviewer can read what it did:
    // the one artefact a failure hands you was misquoting the source.
    //
    // An INTERPOLATION inside the literal is code again, so `it` inside `{...}` is still replaced,
    // and `\{` is an escaped brace that stays text — the same rule the lexer applies, kept in step
    // with it deliberately, because the two disagreeing is how this class of bug starts.
    //
    // That branch is currently unreachable and is written anyway, which is worth saying rather
    // than leaving for someone to discover: a bare `it` inside a string INTERPOLATION does not
    // resolve today — `[it != "v{it}"]` is refused with *unknown variable: it*, because the
    // subject is installed for the clause and not re-installed when the lexer re-enters expression
    // parsing inside a literal. That is a separate gap and not this one. Handling it here costs
    // four lines and means the message stays right on the day the resolver catches up, instead of
    // becoming wrong in a way nobody would think to re-test.
    let mut in_string = false;
    let mut interpolation = 0usize;
    while i < text.len() {
        let c = bytes[i] as char;
        if in_string && interpolation == 0 {
            // `\"` and `\{` do not end the string or open code; copy both bytes and move on.
            if c == '\\' && i + 1 < text.len() {
                out.push(c);
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
                out.push(c);
                i += 1;
                continue;
            }
            if c == '{' {
                interpolation = 1;
                out.push(c);
                i += 1;
                continue;
            }
            // Ordinary text inside the literal: copied through untouched, which is the fix.
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if !in_string && c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if in_string && interpolation > 0 {
            if c == '{' {
                interpolation += 1;
            } else if c == '}' {
                interpolation -= 1;
            }
        }
        if text[i..].starts_with(word) {
            let before_ok = i == 0 || !ident(bytes[i - 1] as char);
            let after = i + word.len();
            let after_ok = after >= text.len() || !ident(bytes[after] as char);
            if before_ok && after_ok {
                out.push_str(with);
                i = after;
                continue;
            }
        }
        // Push one CHARACTER, not one byte: a clause may hold any UTF-8, and advancing by a byte
        // would split a multi-byte one and produce a String that is not valid text.
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}
