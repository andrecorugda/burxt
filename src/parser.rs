//! Parser: tokens -> AST via straightforward recursive descent.
//!
//! Grammar:
//!   program := (fn | stmt)*
//!   fn      := "fn" IDENT "(" (param ("," param)*)? ")" "->" type block
//!   param   := IDENT ":" type
//!   block   := "{" stmt* "}"
//!   stmt    := "let" IDENT ":" type "=" expr ";"
//!            | "print" "(" expr ")" ";"
//!            | "return" expr ";"
//!            | "if" expr block ("else" (block | if-stmt))?
//!   type    := "Int" | "Bool" | "Decimal" "<" INT ("," rounding)? ">"
//!   rounding:= "RoundHalfEven" | "RoundHalfUp"
//!   expr    := additive (cmp additive)?          -- comparisons don't chain
//!   cmp     := "==" | "!=" | "<" | "<=" | ">" | ">="
//!   additive:= term (("+"|"-") term)*
//!   term    := factor (("*"|"/") factor)*
//!   factor  := INT | DECIMAL | "true" | "false" | IDENT
//!            | IDENT "(" (expr ("," expr)*)? ")" | "(" expr ")"

use crate::ast::*;
use crate::lexer::Token;

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, pos: 0 }
    }

    pub fn parse_program(mut self) -> Result<Program, String> {
        let mut fns = Vec::new();
        let mut stmts = Vec::new();
        while !self.at(&Token::Eof) {
            if self.at(&Token::Fn) {
                fns.push(self.parse_fn()?);
            } else {
                stmts.push(self.parse_stmt()?);
            }
        }
        Ok(Program { fns, stmts })
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
            Err(format!("expected {:?}, found {:?}", t, self.peek()))
        }
    }

    // ---- functions ----

    fn parse_fn(&mut self) -> Result<FnDef, String> {
        self.expect(&Token::Fn)?;
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected a function name after 'fn', found {:?}", other)),
        };
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        if !self.at(&Token::RParen) {
            loop {
                let pname = match self.bump() {
                    Token::Ident(s) => s,
                    other => return Err(format!("expected a parameter name, found {:?}", other)),
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
                 (every Burxt function returns a value), found {:?}",
                name,
                self.peek()
            ));
        }
        self.bump();
        let ret = self.parse_type()?;
        let body = self.parse_block()?;
        Ok(FnDef { name, params, ret, body })
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
            other => Err(format!("expected statement, found {:?}", other)),
        }
    }

    fn parse_return(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Return)?;
        let e = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(Stmt::Return(e))
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::If)?;
        let cond = self.parse_expr()?;
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
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(format!("expected identifier after 'let', found {:?}", other)),
        };
        self.expect(&Token::Colon)?;
        let declared = self.parse_type()?;
        self.expect(&Token::Equals)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(Stmt::Let { name, declared, value })
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
            Err(format!("expected `>` to close Decimal<..>, found {:?}", self.peek()))
        }
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        match self.bump() {
            Token::TyInt => Ok(Type::Int),
            Token::TyBool => Ok(Type::Bool),
            Token::TyDecimal => {
                self.expect(&Token::Lt)?;
                let scale = match self.bump() {
                    Token::Int(n) if n >= 0 => n as u32,
                    other => return Err(format!("expected non-negative scale in Decimal<..>, found {:?}", other)),
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
                                 after the comma in Decimal<..>, found {:?}",
                                other
                            ))
                        }
                    }
                } else {
                    None
                };
                self.expect_type_close()?;
                Ok(Type::Decimal { scale, rounding })
            }
            other => Err(format!("expected a type, found {:?}", other)),
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

    fn parse_factor(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Token::Int(n) => Ok(Expr::IntLit(n)),
            Token::Decimal(unscaled, scale) => Ok(Expr::DecimalLit { unscaled, scale }),
            Token::True => Ok(Expr::BoolLit(true)),
            Token::False => Ok(Expr::BoolLit(false)),
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
                } else {
                    Ok(Expr::Var(s))
                }
            }
            Token::LParen => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            other => Err(format!("expected an expression, found {:?}", other)),
        }
    }
}
