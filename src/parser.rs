//! Parser: tokens -> AST via straightforward recursive descent.
//!
//! Grammar for v0.0.1:
//!   program := stmt*
//!   stmt    := "let" IDENT ":" type "=" expr ";"
//!            | "print" "(" expr ")" ";"
//!   type    := "Int" | "Decimal" "<" INT ("," rounding)? ">"
//!   rounding:= "RoundHalfEven" | "RoundHalfUp"
//!   expr    := term (("+"|"-") term)*
//!   term    := factor (("*"|"/") factor)*
//!   factor  := INT | DECIMAL | IDENT | "(" expr ")"

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
        let mut stmts = Vec::new();
        while !self.at(&Token::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        Ok(Program { stmts })
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

    // ---- statements ----

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek() {
            Token::Let => self.parse_let(),
            Token::Print => self.parse_print(),
            other => Err(format!("expected statement, found {:?}", other)),
        }
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

    fn parse_type(&mut self) -> Result<Type, String> {
        match self.bump() {
            Token::TyInt => Ok(Type::Int),
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
                self.expect(&Token::Gt)?;
                Ok(Type::Decimal { scale, rounding })
            }
            other => Err(format!("expected a type, found {:?}", other)),
        }
    }

    // ---- expressions (precedence climbing) ----

    fn parse_expr(&mut self) -> Result<Expr, String> {
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
            Token::Ident(s) => Ok(Expr::Var(s)),
            Token::LParen => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            other => Err(format!("expected an expression, found {:?}", other)),
        }
    }
}
