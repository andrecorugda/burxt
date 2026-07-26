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
use crate::lexer::Token;

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// Struct literals are not allowed directly in an if/while condition —
    /// `while count { ... }` must parse `{` as the loop body, not a literal.
    /// Parenthesizing re-enables them.
    allow_struct_lit: bool,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, pos: 0, allow_struct_lit: true }
    }

    pub fn parse_program(mut self) -> Result<Program, String> {
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
            } else if self.at(&Token::Fn) {
                // `fn (self: T) name(...)` is a method; `fn name(...)` is a
                // free function — the `(` right after `fn` is the tell.
                if self.peek_at(1) == &Token::LParen {
                    methods.push(self.parse_method()?);
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
        Ok(StructDef { name, fields })
    }

    /// `enum Name { Unit, WithPayload(Int, String), }`
    fn parse_enum(&mut self) -> Result<EnumDef, String> {
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
                    if self.at(&Token::Comma) {
                        self.bump();
                    } else {
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
        Ok(EnumDef { name, variants })
    }

    // ---- traits and impls ----

    /// `trait Name { fn m(self) -> T   fn n(mut self, x: U) -> V }`
    /// Signatures only: no bodies, no fields, no default methods.
    fn parse_trait(&mut self) -> Result<TraitDef, String> {
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
        Ok(TraitDef { name, methods })
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
            methods.push(self.parse_method()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(ImplBlock { trait_name, type_name, methods })
    }

    // ---- functions ----

    fn parse_extern(&mut self) -> Result<ExternFn, String> {
        self.expect(&Token::Extern)?;
        self.expect(&Token::Fn)?;
        let (name, params, ret) = self.parse_fn_signature()?;
        self.expect(&Token::Semicolon)?;
        Ok(ExternFn { name, params, ret })
    }

    fn parse_fn(&mut self) -> Result<FnDef, String> {
        self.expect(&Token::Fn)?;
        let (name, params, ret) = self.parse_fn_signature()?;
        let body = self.parse_block()?;
        Ok(FnDef { name, params, ret, body })
    }

    /// `fn (self: Type) name(params) -> ret { body }`, or `fn (mut self: ...)`
    /// for a mutating method.
    fn parse_method(&mut self) -> Result<MethodDef, String> {
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
        self.expect(&Token::Colon)?;
        let receiver = match self.bump() {
            Token::Ident(s) => s,
            other => {
                return Err(format!(
                    "expected a struct name after `self:`, found {}",
                    other.describe()
                ))
            }
        };
        self.expect(&Token::RParen)?;
        let (name, params, ret) = self.parse_fn_signature()?;
        let body = self.parse_block()?;
        Ok(MethodDef { receiver, receiver_mut, name, params, ret, body })
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

    fn parse_fn_signature(&mut self) -> Result<(String, Vec<Param>, Type), String> {
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected a function name after 'fn', found {}", other.describe())),
        };
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
                if self.at(&Token::Comma) {
                    self.bump();
                } else {
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
        Ok((name, params, ret))
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

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek() {
            Token::Let => self.parse_let(),
            Token::Print => self.parse_print(),
            Token::Return => self.parse_return(),
            Token::If => self.parse_if(),
            Token::While => self.parse_while(),
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
    fn parse_assign(&mut self) -> Result<Stmt, String> {
        let name = match self.bump() {
            Token::Ident(s) => s,
            Token::SelfKw => "self".to_string(),
            other => return Err(format!("expected identifier, found {}", other.describe())),
        };

        if self.at(&Token::LParen) {
            let args = self.parse_call_args()?;
            self.expect(&Token::Semicolon)?;
            return Ok(Stmt::ExprStmt(Expr::Call { name, args }));
        }

        if self.at(&Token::LBracket) {
            self.bump();
            let index = self.parse_expr()?;
            self.expect(&Token::RBracket)?;
            self.expect(&Token::Equals)?;
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            return Ok(Stmt::AssignIndex { name, index, value });
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
                let mut base = Expr::Var(name);
                for f in path {
                    base = Expr::Field { base: Box::new(base), field: f };
                }
                return Ok(Stmt::ExprStmt(Expr::MethodCall { base: Box::new(base), method: seg, args }));
            }
            path.push(seg);
        }

        if self.at(&Token::LBracket) && !path.is_empty() {
            self.bump();
            let index = self.parse_expr()?;
            self.expect(&Token::RBracket)?;
            self.expect(&Token::Equals)?;
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            return Ok(Stmt::AssignFieldIndex { name, path, index, value });
        }
        self.expect(&Token::Equals)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        if path.is_empty() {
            Ok(Stmt::Assign { name, value })
        } else {
            Ok(Stmt::AssignField { name, path, value })
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
                if self.at(&Token::Comma) {
                    self.bump();
                } else {
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
    fn parse_match(&mut self) -> Result<Stmt, String> {
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
                    if self.at(&Token::Comma) {
                        self.bump();
                    } else {
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
        Ok(Stmt::Match { value, arms })
    }

    /// `region name { ... }`
    fn parse_region(&mut self) -> Result<Stmt, String> {
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
        Ok(Stmt::Region { name, body })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::While)?;
        let cond = self.parse_cond()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body })
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

    fn parse_return(&mut self) -> Result<Stmt, String> {
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
            if !matches!(e, Expr::Call { .. }) {
                return Err(
                    "`return tail` must be followed by a call — a tail call is a \
                     call that replaces this frame, so there has to be one."
                        .to_string(),
                );
            }
            return Ok(Stmt::TailReturn(e));
        }
        Ok(Stmt::Return(e))
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::If)?;
        let cond = self.parse_cond()?;
        let then_block = self.parse_block()?;
        let else_block = if self.at(&Token::Else) {
            self.bump();
            if self.at(&Token::If) {
                // `else if ...` chains as an else-block holding one if-statement.
                Some(vec![self.parse_if()?])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If { cond, then_block, else_block })
    }

    fn parse_let(&mut self) -> Result<Stmt, String> {
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
        self.expect(&Token::Colon)?;
        let declared = self.parse_type()?;
        self.expect(&Token::Equals)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(Stmt::Let { name, mutable, declared, value })
    }

    fn parse_print(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Print)?;
        self.expect(&Token::LParen)?;
        let e = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Semicolon)?;
        Ok(Stmt::Print(e))
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
            Token::Ident(name) => Ok(Type::Named(name)),
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
        let mut lhs = self.parse_and()?;
        while self.at(&Token::PipePipe) {
            self.bump();
            let rhs = self.parse_and()?;
            lhs = Expr::Logical {
                op: LogicalOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_comparison()?;
        while self.at(&Token::AmpAmp) {
            self.bump();
            let rhs = self.parse_comparison()?;
            lhs = Expr::Logical {
                op: LogicalOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
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
        Ok(Expr::Compare { op, lhs: Box::new(lhs), rhs: Box::new(rhs) })
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_term()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_factor()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_factor()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    /// A factor is a primary followed by any chain of `.field` accesses and
    /// `.method(args)` calls, optionally negated:
    /// `-item.price` is Neg(Field(item, price)).
    fn parse_factor(&mut self) -> Result<Expr, String> {
        if self.at(&Token::Minus) {
            self.bump();
            let e = self.parse_factor()?;
            return Ok(Expr::Neg(Box::new(e)));
        }
        if self.at(&Token::Bang) {
            self.bump();
            let e = self.parse_factor()?;
            return Ok(Expr::Not(Box::new(e)));
        }
        let mut e = self.parse_primary()?;
        loop {
            if self.at(&Token::LBracket) {
                self.bump();
                let index = self.parse_expr()?;
                self.expect(&Token::RBracket)?;
                e = Expr::Index { base: Box::new(e), index: Box::new(index) };
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
                        if self.at(&Token::Comma) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&Token::RParen)?;
                e = Expr::MethodCall { base: Box::new(e), method: name, args };
            } else {
                e = Expr::Field { base: Box::new(e), field: name };
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Token::Int(n) => Ok(Expr::IntLit(n)),
            Token::Decimal(unscaled, scale) => Ok(Expr::DecimalLit { unscaled, scale }),
            Token::True => Ok(Expr::BoolLit(true)),
            Token::False => Ok(Expr::BoolLit(false)),
            Token::Str(s) => Ok(Expr::StrLit(s)),
            Token::InterpStr(parts) => {
                // Each `{...}` was captured as source text; parse it now, so an
                // interpolated expression obeys exactly the same grammar and
                // type rules as one written outside a string.
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        crate::lexer::StrPart::Lit(text) => out.push(InterpPart::Lit(text)),
                        crate::lexer::StrPart::Expr(src) => {
                            let toks = crate::lexer::Lexer::new(&src).tokenize().map_err(|e| {
                                format!("in the interpolation `{{{}}}`: {}", src.trim(), e)
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
                Ok(Expr::InterpStr(out))
            }
            // `self` reads as a plain variable in expression position — its
            // meaning (and mutability) comes from the receiver clause, not
            // from special-casing here.
            Token::SelfKw => Ok(Expr::Var("self".to_string())),
            Token::Ident(s) => {
                if self.at(&Token::LParen) {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at(&Token::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.at(&Token::Comma) {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::Call { name: s, args })
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
                        self.expect(&Token::Colon)?;
                        let value = self.parse_expr()?;
                        fields.push((fname, value));
                        if self.at(&Token::Comma) {
                            self.bump(); // trailing comma allowed
                        } else {
                            break;
                        }
                    }
                    self.expect(&Token::RBrace)?;
                    Ok(Expr::StructLit { name: s, fields })
                } else {
                    // `name[i]` is handled by the postfix loop, so a bare name
                    // is all that is left here.
                    Ok(Expr::Var(s))
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
                Ok(Expr::ArrayLit(elems))
            }
            Token::LParen => {
                // parentheses re-enable struct literals inside a condition
                let saved = self.allow_struct_lit;
                self.allow_struct_lit = true;
                let e = self.parse_expr();
                self.allow_struct_lit = saved;
                let e = e?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            other => Err(format!("expected an expression, found {}", other.describe())),
        }
    }
}
