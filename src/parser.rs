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
        let mut externs = Vec::new();
        let mut fns = Vec::new();
        let mut stmts = Vec::new();
        while !self.at(&Token::Eof) {
            if self.at(&Token::Struct) {
                structs.push(self.parse_struct()?);
            } else if self.at(&Token::Extern) {
                externs.push(self.parse_extern()?);
            } else if self.at(&Token::Fn) {
                fns.push(self.parse_fn()?);
            } else {
                stmts.push(self.parse_stmt()?);
            }
        }
        Ok(Program { structs, externs, fns, stmts })
    }

    // ---- helpers ----

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
            fields.push(Param { name: fname, ty });
            if self.at(&Token::Comma) {
                self.bump(); // trailing comma allowed
            } else {
                break;
            }
        }
        self.expect(&Token::RBrace)?;
        Ok(StructDef { name, fields })
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
                params.push(Param { name: pname, ty });
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
            Token::Ident(_) => self.parse_assign(),
            other => Err(format!("expected statement, found {}", other.describe())),
        }
    }

    fn parse_assign(&mut self) -> Result<Stmt, String> {
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected identifier, found {}", other.describe())),
        };
        // a[i] = value;
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
            match self.bump() {
                Token::Ident(f) => path.push(f),
                other => return Err(format!("expected a field name after '.', found {}", other.describe())),
            }
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
        let e = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
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
            // [T; N] — fixed-size array
            Token::LBracket => {
                let elem = self.parse_type()?;
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
    fn parse_expr(&mut self) -> Result<Expr, String> {
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

    /// A factor is a primary followed by any chain of `.field` accesses,
    /// optionally negated: `-item.price` is Neg(Field(item, price)).
    fn parse_factor(&mut self) -> Result<Expr, String> {
        if self.at(&Token::Minus) {
            self.bump();
            let e = self.parse_factor()?;
            return Ok(Expr::Neg(Box::new(e)));
        }
        let mut e = self.parse_primary()?;
        while self.at(&Token::Dot) {
            self.bump();
            let field = match self.bump() {
                Token::Ident(f) => f,
                other => return Err(format!("expected a field name after '.', found {}", other.describe())),
            };
            e = Expr::Field { base: Box::new(e), field };
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
                } else if self.at(&Token::LBracket) {
                    self.bump();
                    let index = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    Ok(Expr::Index { name: s, index: Box::new(index) })
                } else {
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
