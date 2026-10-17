// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lexer and Pratt parser for the score expression language.
//!
//! Grammar (lowest to highest precedence): `||`, `&&`, comparison
//! (`< <= > >= == !=`), `+ -`, `* /`, unary `- !`, `^` (right associative).
//! A number literal may be followed by a unit symbol with an optional integer
//! exponent (`350 g`, `2 Wh/km`, `9.81 m/s^2`): the unit binds to the literal
//! only, so `2 Wh/km` is `(2 Wh) / km`. Calls are `name(arg, ...)`.

use super::ScoreError;

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `^`
    Pow,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `&&`
    And,
    /// `||`
    Or,
}

/// Expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A number.
    Num(f64),
    /// A variable, unit or constant.
    Var(String),
    /// Unary minus.
    Neg(Box<Expr>),
    /// Logical not.
    Not(Box<Expr>),
    /// Binary operation.
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// Function call.
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, ScoreError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let text: String = chars[start..i].iter().collect();
            let v = text.parse::<f64>().map_err(|_| ScoreError::Parse {
                pos: start,
                message: format!("bad number '{text}'"),
            })?;
            out.push((Tok::Num(v), start));
        } else if c.is_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push((Tok::Ident(chars[start..i].iter().collect()), start));
        } else {
            let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let tok = match two.as_str() {
                "<=" => Some(Tok::Op("<=")),
                ">=" => Some(Tok::Op(">=")),
                "==" => Some(Tok::Op("==")),
                "!=" => Some(Tok::Op("!=")),
                "&&" => Some(Tok::Op("&&")),
                "||" => Some(Tok::Op("||")),
                _ => None,
            };
            if let Some(t) = tok {
                out.push((t, start));
                i += 2;
                continue;
            }
            let t = match c {
                '+' => Tok::Op("+"),
                '-' => Tok::Op("-"),
                '*' => Tok::Op("*"),
                '/' => Tok::Op("/"),
                '^' => Tok::Op("^"),
                '<' => Tok::Op("<"),
                '>' => Tok::Op(">"),
                '!' => Tok::Op("!"),
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                ',' => Tok::Comma,
                _ => {
                    return Err(ScoreError::Parse {
                        pos: start,
                        message: format!("unexpected character '{c}'"),
                    })
                }
            };
            out.push((t, start));
            i += 1;
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    at: usize,
    len: usize,
}

/// Parse an expression.
pub fn parse(src: &str) -> Result<Expr, ScoreError> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        at: 0,
        len: src.chars().count(),
    };
    let e = p.expr(0)?;
    if p.at < p.toks.len() {
        return Err(p.err("unexpected trailing input"));
    }
    Ok(e)
}

impl Parser {
    fn err(&self, message: &str) -> ScoreError {
        ScoreError::Parse {
            pos: self.toks.get(self.at).map_or(self.len, |t| t.1),
            message: message.to_string(),
        }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at).map(|t| &t.0)
    }

    fn infix(&self) -> Option<(BinOp, u8, u8)> {
        let Some(Tok::Op(op)) = self.peek() else {
            return None;
        };
        // (operator, left binding power, right binding power)
        Some(match *op {
            "||" => (BinOp::Or, 1, 2),
            "&&" => (BinOp::And, 3, 4),
            "<" => (BinOp::Lt, 5, 6),
            "<=" => (BinOp::Le, 5, 6),
            ">" => (BinOp::Gt, 5, 6),
            ">=" => (BinOp::Ge, 5, 6),
            "==" => (BinOp::Eq, 5, 6),
            "!=" => (BinOp::Ne, 5, 6),
            "+" => (BinOp::Add, 7, 8),
            "-" => (BinOp::Sub, 7, 8),
            "*" => (BinOp::Mul, 9, 10),
            "/" => (BinOp::Div, 9, 10),
            "^" => (BinOp::Pow, 14, 13),
            _ => return None,
        })
    }

    fn expr(&mut self, min_bp: u8) -> Result<Expr, ScoreError> {
        let mut lhs = self.prefix()?;
        while let Some((op, lbp, rbp)) = self.infix() {
            if lbp < min_bp {
                break;
            }
            self.at += 1;
            let rhs = self.expr(rbp)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn prefix(&mut self) -> Result<Expr, ScoreError> {
        let Some(tok) = self.peek().cloned() else {
            return Err(self.err("unexpected end of expression"));
        };
        self.at += 1;
        match tok {
            Tok::Num(v) => self.literal_unit(Expr::Num(v)),
            Tok::Op("-") => Ok(Expr::Neg(Box::new(self.expr(11)?))),
            Tok::Op("!") => Ok(Expr::Not(Box::new(self.expr(11)?))),
            Tok::LParen => {
                let e = self.expr(0)?;
                self.expect_rparen()?;
                Ok(e)
            }
            Tok::Ident(name) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.at += 1;
                    let mut args = Vec::new();
                    if self.peek() == Some(&Tok::RParen) {
                        self.at += 1;
                        return Ok(Expr::Call(name, args));
                    }
                    loop {
                        args.push(self.expr(0)?);
                        match self.peek() {
                            Some(Tok::Comma) => self.at += 1,
                            Some(Tok::RParen) => {
                                self.at += 1;
                                break;
                            }
                            _ => return Err(self.err("expected ',' or ')'")),
                        }
                    }
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            _ => {
                self.at -= 1;
                Err(self.err("unexpected token"))
            }
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ScoreError> {
        if self.peek() == Some(&Tok::RParen) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.err("expected ')'"))
        }
    }

    /// `number unit [^ integer]`: the unit binds to the literal only.
    fn literal_unit(&mut self, num: Expr) -> Result<Expr, ScoreError> {
        let Some(Tok::Ident(unit)) = self.peek().cloned() else {
            return Ok(num);
        };
        self.at += 1;
        let mut unit_expr = Expr::Var(unit);
        if self.peek() == Some(&Tok::Op("^")) {
            self.at += 1;
            let negative = if self.peek() == Some(&Tok::Op("-")) {
                self.at += 1;
                true
            } else {
                false
            };
            match self.peek().cloned() {
                Some(Tok::Num(n)) => {
                    self.at += 1;
                    let n = if negative { -n } else { n };
                    unit_expr = Expr::Bin(BinOp::Pow, Box::new(unit_expr), Box::new(Expr::Num(n)));
                }
                _ => return Err(self.err("expected a number after '^' in a unit")),
            }
        }
        Ok(Expr::Bin(BinOp::Mul, Box::new(num), Box::new(unit_expr)))
    }
}
