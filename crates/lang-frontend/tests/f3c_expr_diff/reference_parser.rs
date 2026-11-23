//! Independent Guide v0.47 Chapter 04 18-level expression reference parser (F3c -> F4a).
//!
//! Directly encodes `E-ATOM`, `E-TYPE-SIMPLE`, `E-P01`..`E-P18`, all four non-associative
//! operator groups, call-argument named/`&` context predicates, and Span-adjacency
//! forbidden-operator rejection (`++`, `--`, `<<`, `>>`, `...`) without sharing
//! production parser internals.

use lang_frontend::{
    lexer::{Keyword, LexedFile, LexemeKind, Symbol, TokenKind, lex},
    source::SourceMap,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Accepted(String),
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok<'a> {
    Ident(&'a str),
    Int(&'a str),
    Bool(bool),
    Null,
    This,
    As,
    In,
    Is,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    QDot,
    Question,
    Elvis,
    BangBang,
    Bang,
    ColonColon,
    Star,
    Slash,
    Percent,
    Plus,
    Minus,
    DotDot,
    DotDotLess,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    BangEq,
    Amp,
    AndAnd,
    OrOr,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Eq,
    AsQuestion,
    BangIn,
    BangIs,
    Eof,
}

#[derive(Clone, Copy, Debug)]
struct SpannedTok<'a> {
    tok: Tok<'a>,
    start: usize,
    end: usize,
}

fn to_stream<'a>(sources: &'a SourceMap, lexed: &LexedFile) -> Option<Vec<SpannedTok<'a>>> {
    if !lexed.diagnostics().is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for lexeme in lexed.lexemes() {
        let span = lexeme.span();
        let spelling = sources.slice(span).unwrap();
        let tok = match lexeme.kind() {
            LexemeKind::Trivia(_) => continue,
            LexemeKind::Eof => Tok::Eof,
            LexemeKind::Token(TokenKind::Identifier) => Tok::Ident(spelling),
            LexemeKind::Token(TokenKind::IntegerLiteral(_)) => Tok::Int(spelling),
            LexemeKind::Token(TokenKind::Keyword(Keyword::True)) => Tok::Bool(true),
            LexemeKind::Token(TokenKind::Keyword(Keyword::False)) => Tok::Bool(false),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Null)) => Tok::Null,
            LexemeKind::Token(TokenKind::Keyword(Keyword::This)) => Tok::This,
            LexemeKind::Token(TokenKind::Keyword(Keyword::As)) => Tok::As,
            LexemeKind::Token(TokenKind::Keyword(Keyword::In)) => Tok::In,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Is)) => Tok::Is,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => Tok::LParen,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => Tok::RParen,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => Tok::LBracket,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => Tok::RBracket,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => Tok::Comma,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot)) => Tok::Dot,
            LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionDot)) => Tok::QDot,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Question)) => Tok::Question,
            LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionColon)) => Tok::Elvis,
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangBang)) => Tok::BangBang,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)) => Tok::Bang,
            LexemeKind::Token(TokenKind::Symbol(Symbol::ColonColon)) => Tok::ColonColon,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Star)) => Tok::Star,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Slash)) => Tok::Slash,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Percent)) => Tok::Percent,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => Tok::Plus,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => Tok::Minus,
            LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot)) => Tok::DotDot,
            LexemeKind::Token(TokenKind::Symbol(Symbol::DotDotLess)) => Tok::DotDotLess,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => Tok::Lt,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => Tok::Gt,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LessEqual)) => Tok::Le,
            LexemeKind::Token(TokenKind::Symbol(Symbol::GreaterEqual)) => Tok::Ge,
            LexemeKind::Token(TokenKind::Symbol(Symbol::EqualEqual)) => Tok::EqEq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangEqual)) => Tok::BangEq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Ampersand)) => Tok::Amp,
            LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)) => Tok::AndAnd,
            LexemeKind::Token(TokenKind::Symbol(Symbol::OrOr)) => Tok::OrOr,
            LexemeKind::Token(TokenKind::Symbol(Symbol::PlusEqual)) => Tok::PlusEq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::MinusEqual)) => Tok::MinusEq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::StarEqual)) => Tok::StarEq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::SlashEqual)) => Tok::SlashEq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::PercentEqual)) => Tok::PercentEq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => Tok::Eq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)) => Tok::AsQuestion,
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)) => Tok::BangIn,
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)) => Tok::BangIs,
            other => panic!("unexpected token in F3c subset: {other:?} ({spelling:?})"),
        };
        out.push(SpannedTok {
            tok,
            start: span.start(),
            end: span.end(),
        });
    }
    Some(out)
}

/// Guide 04: no-trivia adjacent `++`, `--`, `<<`, `>>`, `...` are rejected as unsupported operators.
fn has_adjacent_forbidden_operator(tokens: &[SpannedTok<'_>]) -> bool {
    tokens.windows(2).any(|w| {
        w[0].end == w[1].start
            && matches!(
                (w[0].tok, w[1].tok),
                (Tok::Plus, Tok::Plus)
                    | (Tok::Minus, Tok::Minus)
                    | (Tok::Lt, Tok::Lt)
                    | (Tok::Gt, Tok::Gt)
                    | (Tok::DotDot, Tok::Dot)
                    | (Tok::Dot, Tok::DotDot)
            )
    })
}

struct RefParser<'a> {
    tokens: &'a [SpannedTok<'a>],
    pos: usize,
}

impl<'a> RefParser<'a> {
    fn new(tokens: &'a [SpannedTok<'a>]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Tok<'a> {
        self.tokens[self.pos].tok
    }

    fn peek_at(&self, offset: usize) -> Tok<'a> {
        self.tokens
            .get(self.pos + offset)
            .map_or(Tok::Eof, |t| t.tok)
    }

    fn expect(&mut self, want: Tok<'a>) -> Result<(), ()> {
        if self.peek() == want {
            self.pos += 1;
            Ok(())
        } else {
            Err(())
        }
    }

    fn expect_ident(&mut self) -> Result<&'a str, ()> {
        match self.peek() {
            Tok::Ident(name) => {
                self.pos += 1;
                Ok(name)
            }
            _ => Err(()),
        }
    }

    fn parse_root(&mut self) -> Result<String, ()> {
        let expr = self.parse_p18_assignment()?;
        self.expect(Tok::Eof)?;
        Ok(expr)
    }

    // E-P18: `=`, `+=`, `-=`, `*=`, `/=`, `%=` (right-associative)
    fn parse_p18_assignment(&mut self) -> Result<String, ()> {
        let lhs = self.parse_p17_or()?;
        let op = match self.peek() {
            Tok::Eq => "=",
            Tok::PlusEq => "+=",
            Tok::MinusEq => "-=",
            Tok::StarEq => "*=",
            Tok::SlashEq => "/=",
            Tok::PercentEq => "%=",
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.parse_p18_assignment()?;
        Ok(format!("({op} {lhs} {rhs})"))
    }

    // E-P17: `||` (left-associative)
    fn parse_p17_or(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p16_and()?;
        while self.peek() == Tok::OrOr {
            self.pos += 1;
            let rhs = self.parse_p16_and()?;
            lhs = format!("(|| {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P16: `&&` (left-associative)
    fn parse_p16_and(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p15_equality()?;
        while self.peek() == Tok::AndAnd {
            self.pos += 1;
            let rhs = self.parse_p15_equality()?;
            lhs = format!("(&& {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P15: `==`, `!=` (NON-ASSOCIATIVE)
    fn parse_p15_equality(&mut self) -> Result<String, ()> {
        let lhs = self.parse_p14_comparison()?;
        let op = match self.peek() {
            Tok::EqEq => "==",
            Tok::BangEq => "!=",
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.parse_p14_comparison()?;
        if matches!(self.peek(), Tok::EqEq | Tok::BangEq) {
            return Err(());
        }
        Ok(format!("({op} {lhs} {rhs})"))
    }

    // E-P14: `<`, `>`, `<=`, `>=` (NON-ASSOCIATIVE)
    fn parse_p14_comparison(&mut self) -> Result<String, ()> {
        let lhs = self.parse_p13_containment_typetest()?;
        let op = match self.peek() {
            Tok::Lt => "<",
            Tok::Gt => ">",
            Tok::Le => "<=",
            Tok::Ge => ">=",
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.parse_p13_containment_typetest()?;
        if matches!(self.peek(), Tok::Lt | Tok::Gt | Tok::Le | Tok::Ge) {
            return Err(());
        }
        Ok(format!("({op} {lhs} {rhs})"))
    }

    // E-P13: `in`, `!in`, `is`, `!is` (NON-ASSOCIATIVE, mutually exclusive)
    fn parse_p13_containment_typetest(&mut self) -> Result<String, ()> {
        let lhs = self.parse_p12_elvis()?;
        let res = match self.peek() {
            Tok::In => {
                self.pos += 1;
                let rhs = self.parse_p12_elvis()?;
                format!("(in {lhs} {rhs})")
            }
            Tok::BangIn => {
                self.pos += 1;
                let rhs = self.parse_p12_elvis()?;
                format!("(!in {lhs} {rhs})")
            }
            Tok::Is => {
                self.pos += 1;
                let ty = self.parse_simple_type()?;
                format!("(is {lhs} {ty})")
            }
            Tok::BangIs => {
                self.pos += 1;
                let ty = self.parse_simple_type()?;
                format!("(!is {lhs} {ty})")
            }
            _ => return Ok(lhs),
        };
        if matches!(self.peek(), Tok::In | Tok::BangIn | Tok::Is | Tok::BangIs) {
            return Err(());
        }
        Ok(res)
    }

    // E-P12: `?:` (RIGHT-ASSOCIATIVE)
    fn parse_p12_elvis(&mut self) -> Result<String, ()> {
        let lhs = self.parse_p11_to()?;
        if self.peek() == Tok::Elvis {
            self.pos += 1;
            let rhs = self.parse_p12_elvis()?;
            Ok(format!("(?: {lhs} {rhs})"))
        } else {
            Ok(lhs)
        }
    }

    // E-P11: `to` (left-associative soft keyword)
    fn parse_p11_to(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p10_range()?;
        while self.peek() == Tok::Ident("to") {
            self.pos += 1;
            let rhs = self.parse_p10_range()?;
            lhs = format!("(to {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P10: `..`, `..<` (NON-ASSOCIATIVE)
    fn parse_p10_range(&mut self) -> Result<String, ()> {
        let lhs = self.parse_p09_bitwise_or()?;
        let op = match self.peek() {
            Tok::DotDot => "..",
            Tok::DotDotLess => "..<",
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.parse_p09_bitwise_or()?;
        if matches!(self.peek(), Tok::DotDot | Tok::DotDotLess) {
            return Err(());
        }
        Ok(format!("({op} {lhs} {rhs})"))
    }

    // E-P09: `or` (left-associative soft keyword)
    fn parse_p09_bitwise_or(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p08_bitwise_xor()?;
        while self.peek() == Tok::Ident("or") {
            self.pos += 1;
            let rhs = self.parse_p08_bitwise_xor()?;
            lhs = format!("(or {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P08: `xor` (left-associative soft keyword)
    fn parse_p08_bitwise_xor(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p07_bitwise_and()?;
        while self.peek() == Tok::Ident("xor") {
            self.pos += 1;
            let rhs = self.parse_p07_bitwise_and()?;
            lhs = format!("(xor {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P07: `and` (left-associative soft keyword)
    fn parse_p07_bitwise_and(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p06_shift()?;
        while self.peek() == Tok::Ident("and") {
            self.pos += 1;
            let rhs = self.parse_p06_shift()?;
            lhs = format!("(and {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P06: `shl`, `shr`, `ushr` (left-associative soft keywords)
    fn parse_p06_shift(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p05_additive()?;
        while let Tok::Ident(op @ ("shl" | "shr" | "ushr")) = self.peek() {
            self.pos += 1;
            let rhs = self.parse_p05_additive()?;
            lhs = format!("({op} {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P05: `+`, `-` (left-associative)
    fn parse_p05_additive(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p04_multiplicative()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => "+",
                Tok::Minus => "-",
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_p04_multiplicative()?;
            lhs = format!("({op} {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P04: `*`, `/`, `%` (left-associative)
    fn parse_p04_multiplicative(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p03_cast()?;
        loop {
            let op = match self.peek() {
                Tok::Star => "*",
                Tok::Slash => "/",
                Tok::Percent => "%",
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_p03_cast()?;
            lhs = format!("({op} {lhs} {rhs})");
        }
        Ok(lhs)
    }

    // E-P03: `as`, `as?` (left-associative)
    fn parse_p03_cast(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_p02_prefix()?;
        loop {
            let op = match self.peek() {
                Tok::As => "as",
                Tok::AsQuestion => "as?",
                _ => break,
            };
            self.pos += 1;
            let ty = self.parse_simple_type()?;
            lhs = format!("({op} {lhs} {ty})");
        }
        Ok(lhs)
    }

    // E-P02: prefix `!`, `+`, `-` (right-associative)
    fn parse_p02_prefix(&mut self) -> Result<String, ()> {
        let op = match self.peek() {
            Tok::Bang => "pre!",
            Tok::Plus => "pre+",
            Tok::Minus => "pre-",
            _ => return self.parse_p01_postfix(),
        };
        self.pos += 1;
        let operand = self.parse_p02_prefix()?;
        Ok(format!("({op} {operand})"))
    }

    // E-P01: `. ID`, `?. ID`, `( args? )`, `[ expr ]`, `!!`, `?`, `:: ID`
    fn parse_p01_postfix(&mut self) -> Result<String, ()> {
        let mut expr = self.parse_atom()?;
        loop {
            match self.peek() {
                Tok::Dot => {
                    self.pos += 1;
                    let name = self.expect_ident()?;
                    expr = format!("(. {expr} {name})");
                }
                Tok::QDot => {
                    self.pos += 1;
                    let name = self.expect_ident()?;
                    expr = format!("(?. {expr} {name})");
                }
                Tok::LParen => {
                    let args = self.parse_call_args()?;
                    if args.is_empty() {
                        expr = format!("(call {expr})");
                    } else {
                        expr = format!("(call {expr} {})", args.join(" "));
                    }
                }
                Tok::LBracket => {
                    self.pos += 1;
                    let idx = self.parse_p18_assignment()?;
                    self.expect(Tok::RBracket)?;
                    expr = format!("(idx {expr} {idx})");
                }
                Tok::BangBang => {
                    self.pos += 1;
                    expr = format!("(!! {expr})");
                }
                Tok::Question => {
                    self.pos += 1;
                    expr = format!("(? {expr})");
                }
                Tok::ColonColon => {
                    self.pos += 1;
                    let name = self.expect_ident()?;
                    expr = format!("(:: {expr} {name})");
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<String>, ()> {
        self.expect(Tok::LParen)?;
        let mut args = Vec::new();
        if self.peek() == Tok::RParen {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            let named = if let Tok::Ident(n) = self.peek()
                && self.peek_at(1) == Tok::Eq
            {
                self.pos += 2;
                Some(n)
            } else {
                None
            };
            let is_inout = if self.peek() == Tok::Amp {
                self.pos += 1;
                true
            } else {
                false
            };
            let val = if is_inout {
                // Guide 07: `&` binds below assignment; `f(&x = y)` is rejected while `f(&(x = y))` is valid.
                self.parse_p17_or()?
            } else {
                self.parse_p18_assignment()?
            };
            let mut rendered = if is_inout { format!("&{val}") } else { val };
            if let Some(n) = named {
                rendered = format!("(named {n} {rendered})");
            }
            args.push(rendered);
            if self.peek() == Tok::Comma {
                self.pos += 1;
                continue;
            }
            self.expect(Tok::RParen)?;
            break;
        }
        Ok(args)
    }

    fn parse_simple_type(&mut self) -> Result<String, ()> {
        let mut name = self.expect_ident()?.to_owned();
        if self.peek() == Tok::Question {
            self.pos += 1;
            name.push('?');
        }
        Ok(name)
    }

    fn parse_atom(&mut self) -> Result<String, ()> {
        match self.peek() {
            Tok::Ident(n) => {
                self.pos += 1;
                Ok(n.to_owned())
            }
            Tok::Int(n) => {
                self.pos += 1;
                Ok(n.to_owned())
            }
            Tok::Bool(b) => {
                self.pos += 1;
                Ok(b.to_string())
            }
            Tok::Null => {
                self.pos += 1;
                Ok("null".to_owned())
            }
            Tok::This => {
                self.pos += 1;
                Ok("this".to_owned())
            }
            Tok::ColonColon => {
                self.pos += 1;
                let name = self.expect_ident()?;
                Ok(format!("(:: {name})"))
            }
            Tok::LParen => {
                self.pos += 1;
                let inner = self.parse_p18_assignment()?;
                self.expect(Tok::RParen)?;
                Ok(format!("(group {inner})"))
            }
            _ => Err(()),
        }
    }
}

pub fn run_expression_reference(source: &str) -> Outcome {
    let mut sources = SourceMap::new();
    let id = sources.add_source("ref-expr.ko", source).unwrap();
    let lexed = lex(&sources, id).unwrap();
    let Some(tokens) = to_stream(&sources, &lexed) else {
        return Outcome::Rejected;
    };
    if has_adjacent_forbidden_operator(&tokens) {
        return Outcome::Rejected;
    }
    let mut parser = RefParser::new(&tokens);
    match parser.parse_root() {
        Ok(sexpr) => Outcome::Accepted(sexpr),
        Err(()) => Outcome::Rejected,
    }
}
