//! Independent Guide v0.47 + SPEC-0479 reference parser for the F3d high-risk slice.
//!
//! Consumes only the public `LexedFile` token + physical `NL` event stream without
//! calling any production parser internals.

use lang_frontend::{
    lexer::{Keyword, LexedFile, LexemeKind, Symbol, TokenKind, TriviaKind, lex},
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
    Val,
    If,
    Else,
    When,
    Return,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Dot,
    Semi,
    Arrow,
    Eq,
    Plus,
    Minus,
    Bang,
    Lt,
    Gt,
    AndAnd,
    OrOr,
    Question,
    Nl,
    Eof,
}

fn to_stream<'a>(sources: &'a SourceMap, lexed: &LexedFile) -> Option<Vec<Tok<'a>>> {
    if !lexed.diagnostics().is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for lexeme in lexed.lexemes() {
        let spelling = sources.slice(lexeme.span()).unwrap();
        let tok = match lexeme.kind() {
            LexemeKind::Trivia(TriviaKind::Newline | TriviaKind::BlockComment) => {
                for &b in spelling.as_bytes() {
                    if b == b'\n' {
                        out.push(Tok::Nl);
                    }
                }
                continue;
            }
            LexemeKind::Trivia(_) => continue,
            LexemeKind::Eof => Tok::Eof,
            LexemeKind::Token(TokenKind::Identifier) => Tok::Ident(spelling),
            LexemeKind::Token(TokenKind::IntegerLiteral(_)) => Tok::Int(spelling),
            LexemeKind::Token(TokenKind::Keyword(Keyword::True)) => Tok::Bool(true),
            LexemeKind::Token(TokenKind::Keyword(Keyword::False)) => Tok::Bool(false),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Val)) => Tok::Val,
            LexemeKind::Token(TokenKind::Keyword(Keyword::If)) => Tok::If,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Else)) => Tok::Else,
            LexemeKind::Token(TokenKind::Keyword(Keyword::When)) => Tok::When,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Return)) => Tok::Return,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => Tok::LParen,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => Tok::RParen,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => Tok::LBrace,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => Tok::RBrace,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => Tok::Comma,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot)) => Tok::Dot,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) => Tok::Semi,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)) => Tok::Arrow,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => Tok::Eq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => Tok::Plus,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => Tok::Minus,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)) => Tok::Bang,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => Tok::Lt,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => Tok::Gt,
            LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)) => Tok::AndAnd,
            LexemeKind::Token(TokenKind::Symbol(Symbol::OrOr)) => Tok::OrOr,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Question)) => Tok::Question,
            other => panic!("unexpected token in F3d subset: {other:?} ({spelling:?})"),
        };
        out.push(tok);
    }
    Some(out)
}

pub fn format_call(callee: &str, type_args: &[String], args: &[String]) -> String {
    let mut parts = vec![callee.to_owned()];
    if !type_args.is_empty() {
        parts.push(format!("<{}>", type_args.join(",")));
    }
    parts.extend(args.iter().cloned());
    format!("(call {})", parts.join(" "))
}

struct RefParser<'a> {
    tokens: &'a [Tok<'a>],
    pos: usize,
}

impl<'a> RefParser<'a> {
    fn new(tokens: &'a [Tok<'a>]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Tok<'a> {
        self.tokens[self.pos]
    }

    fn next_non_nl(&self) -> (usize, Tok<'a>) {
        let mut i = self.pos;
        while matches!(self.tokens[i], Tok::Nl) {
            i += 1;
        }
        (i, self.tokens[i])
    }

    fn skip_nl(&mut self) -> bool {
        let mut saw = false;
        while matches!(self.peek(), Tok::Nl) {
            self.pos += 1;
            saw = true;
        }
        saw
    }

    fn expect(&mut self, want: Tok<'a>) -> Result<(), ()> {
        if self.peek() == want {
            self.pos += 1;
            Ok(())
        } else {
            Err(())
        }
    }

    fn parse_root_block(&mut self) -> Result<String, ()> {
        self.skip_nl();
        let out = self.parse_braced_body("block")?;
        self.skip_nl();
        self.expect(Tok::Eof)?;
        Ok(out)
    }

    fn parse_braced_body(&mut self, tag: &str) -> Result<String, ()> {
        self.expect(Tok::LBrace)?;
        let mut parts = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            parts.push(self.parse_statement(false)?);
            let mut separated = false;
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
                separated = true;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            if !separated {
                return Err(());
            }
        }
        if parts.is_empty() {
            Ok(format!("({tag})"))
        } else {
            Ok(format!("({tag} {})", parts.join(" ")))
        }
    }

    fn parse_statement(&mut self, in_when_body: bool) -> Result<String, ()> {
        match self.peek() {
            Tok::Val => {
                self.pos += 1;
                self.skip_nl();
                let Tok::Ident(name) = self.peek() else {
                    return Err(());
                };
                self.pos += 1;
                self.skip_nl();
                self.expect(Tok::Eq)?;
                self.skip_nl();
                let init = self.parse_expr(in_when_body)?;
                Ok(format!("(val {name} {init})"))
            }
            Tok::LBrace => Err(()),
            Tok::Ident("block") if self.tokens.get(self.pos + 1) == Some(&Tok::LBrace) => {
                self.pos += 1;
                self.parse_braced_body("nested-block")
            }
            _ => self.parse_expr(in_when_body),
        }
    }

    fn parse_expr(&mut self, in_when_body: bool) -> Result<String, ()> {
        self.parse_or(in_when_body)
    }

    fn parse_or(&mut self, in_when_body: bool) -> Result<String, ()> {
        let mut lhs = self.parse_and(in_when_body)?;
        loop {
            let saved = self.pos;
            self.skip_nl();
            if self.peek() == Tok::OrOr {
                self.pos += 1;
                self.skip_nl();
                let rhs = self.parse_and(in_when_body)?;
                lhs = format!("(or {lhs} {rhs})");
            } else {
                self.pos = saved;
                break;
            }
        }
        Ok(lhs)
    }

    fn parse_and(&mut self, in_when_body: bool) -> Result<String, ()> {
        let mut lhs = self.parse_cmp(in_when_body)?;
        loop {
            let saved = self.pos;
            self.skip_nl();
            if self.peek() == Tok::AndAnd {
                self.pos += 1;
                self.skip_nl();
                let rhs = self.parse_cmp(in_when_body)?;
                lhs = format!("(and {lhs} {rhs})");
            } else {
                self.pos = saved;
                break;
            }
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self, in_when_body: bool) -> Result<String, ()> {
        let lhs = self.parse_add(in_when_body)?;
        let saved = self.pos;
        self.skip_nl();
        let op = match self.peek() {
            Tok::Lt => "lt",
            Tok::Gt => "gt",
            _ => {
                self.pos = saved;
                return Ok(lhs);
            }
        };
        self.pos += 1;
        self.skip_nl();
        let rhs = self.parse_add(in_when_body)?;
        let after = self.pos;
        self.skip_nl();
        if matches!(self.peek(), Tok::Lt | Tok::Gt) {
            return Err(());
        }
        self.pos = after;
        Ok(format!("({op} {lhs} {rhs})"))
    }

    fn parse_add(&mut self, in_when_body: bool) -> Result<String, ()> {
        let mut lhs = self.parse_prefix(in_when_body)?;
        loop {
            match self.peek() {
                Tok::Plus => {
                    self.pos += 1;
                    self.skip_nl();
                    let rhs = self.parse_prefix(in_when_body)?;
                    lhs = format!("(add {lhs} {rhs})");
                }
                Tok::Minus => {
                    self.pos += 1;
                    self.skip_nl();
                    let rhs = self.parse_prefix(in_when_body)?;
                    lhs = format!("(sub {lhs} {rhs})");
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self, in_when_body: bool) -> Result<String, ()> {
        match self.peek() {
            Tok::Minus => {
                self.pos += 1;
                let operand = self.parse_prefix(in_when_body)?;
                Ok(format!("(neg {operand})"))
            }
            Tok::Bang => {
                self.pos += 1;
                let operand = self.parse_prefix(in_when_body)?;
                Ok(format!("(not {operand})"))
            }
            _ => self.parse_postfix(in_when_body),
        }
    }

    fn parse_postfix(&mut self, in_when_body: bool) -> Result<String, ()> {
        let mut expr = self.parse_primary()?;
        let mut is_bare_block_name = expr == "(id block)";
        loop {
            let saved = self.pos;
            self.skip_nl();
            if self.peek() == Tok::Dot {
                self.pos += 1;
                self.skip_nl();
                let Tok::Ident(name) = self.peek() else {
                    return Err(());
                };
                self.pos += 1;
                expr = format!("(member {expr} (id {name}))");
                is_bare_block_name = false;
                continue;
            }
            self.pos = saved;

            if self.peek() == Tok::Lt
                && let Some((type_args, after_gt)) = self.try_type_args()
            {
                let next_after_gt = self.tokens[after_gt];
                if next_after_gt == Tok::LParen {
                    self.pos = after_gt;
                    let mut args = self.parse_arg_list()?;
                    if self.should_attach_crossline_trailing_lambda(in_when_body) {
                        self.skip_nl();
                        args.push(self.parse_lambda(false)?);
                    }
                    expr = format_call(&expr, &type_args, &args);
                    is_bare_block_name = false;
                    continue;
                }
                if next_after_gt == Tok::LBrace {
                    self.pos = after_gt;
                    let lambda = self.parse_lambda(false)?;
                    expr = format_call(&expr, &type_args, &[lambda]);
                    is_bare_block_name = false;
                    continue;
                }
            }

            if self.peek() == Tok::LParen {
                let mut args = self.parse_arg_list()?;
                if self.should_attach_crossline_trailing_lambda(in_when_body) {
                    self.skip_nl();
                    args.push(self.parse_lambda(false)?);
                }
                expr = format_call(&expr, &[], &args);
                is_bare_block_name = false;
                continue;
            }

            if self.peek() == Tok::LBrace {
                if is_bare_block_name {
                    return Err(());
                }
                let lambda = self.parse_lambda(false)?;
                expr = format_call(&expr, &[], &[lambda]);
                is_bare_block_name = false;
                continue;
            }

            break;
        }
        Ok(expr)
    }

    fn should_attach_crossline_trailing_lambda(&self, in_when_body: bool) -> bool {
        if self.peek() == Tok::LBrace {
            return true;
        }
        let (idx, tok) = self.next_non_nl();
        if idx == self.pos || tok != Tok::LBrace {
            return false;
        }
        if in_when_body && self.line_starts_when_condition(idx) {
            return false;
        }
        true
    }

    fn line_starts_when_condition(&self, start: usize) -> bool {
        let mut i = start;
        let mut depth = 0usize;
        while i < self.tokens.len() {
            match self.tokens[i] {
                Tok::LBrace | Tok::LParen => depth += 1,
                Tok::RBrace | Tok::RParen => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                Tok::Arrow if depth == 0 => return true,
                Tok::Nl | Tok::Semi | Tok::Eof if depth == 0 => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn try_type_args(&self) -> Option<(Vec<String>, usize)> {
        let mut p = RefParser {
            tokens: self.tokens,
            pos: self.pos + 1,
        };
        let mut args = Vec::new();
        loop {
            p.skip_nl();
            args.push(p.parse_type_ref().ok()?);
            p.skip_nl();
            if p.peek() == Tok::Comma {
                p.pos += 1;
                continue;
            }
            if p.peek() == Tok::Gt {
                p.pos += 1;
                return Some((args, p.pos));
            }
            return None;
        }
    }

    fn parse_type_ref(&mut self) -> Result<String, ()> {
        let Tok::Ident(name) = self.peek() else {
            return Err(());
        };
        self.pos += 1;
        let mut out = name.to_owned();
        if self.peek() == Tok::Lt {
            self.pos += 1;
            let mut inner = Vec::new();
            loop {
                self.skip_nl();
                inner.push(self.parse_type_ref()?);
                self.skip_nl();
                if self.peek() == Tok::Comma {
                    self.pos += 1;
                    continue;
                }
                self.expect(Tok::Gt)?;
                break;
            }
            out = format!("{out}<{}>", inner.join(","));
        }
        if self.peek() == Tok::Question {
            self.pos += 1;
            out.push('?');
        }
        Ok(out)
    }

    fn parse_arg_list(&mut self) -> Result<Vec<String>, ()> {
        self.expect(Tok::LParen)?;
        let mut args = Vec::new();
        self.skip_nl();
        if self.peek() == Tok::RParen {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            self.skip_nl();
            args.push(self.parse_expr(false)?);
            self.skip_nl();
            if self.peek() == Tok::Comma {
                self.pos += 1;
                self.skip_nl();
                if self.peek() == Tok::RParen {
                    self.pos += 1;
                    break;
                }
                continue;
            }
            self.expect(Tok::RParen)?;
            break;
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<String, ()> {
        match self.peek() {
            Tok::Ident("move") if self.next_after_one_is_lbrace() => {
                self.pos += 1;
                self.skip_nl();
                self.parse_lambda(true)
            }
            Tok::Ident(name) => {
                self.pos += 1;
                Ok(format!("(id {name})"))
            }
            Tok::Int(lit) => {
                self.pos += 1;
                Ok(format!("(int {lit})"))
            }
            Tok::Bool(b) => {
                self.pos += 1;
                Ok(format!("(bool {b})"))
            }
            Tok::LParen => {
                self.pos += 1;
                self.skip_nl();
                let inner = self.parse_expr(false)?;
                self.skip_nl();
                self.expect(Tok::RParen)?;
                Ok(format!("(group {inner})"))
            }
            Tok::LBrace => self.parse_lambda(false),
            Tok::Return => {
                self.pos += 1;
                if matches!(
                    self.peek(),
                    Tok::Nl | Tok::Semi | Tok::RBrace | Tok::RParen | Tok::Comma | Tok::Eof
                ) {
                    Ok("(return)".to_owned())
                } else {
                    let val = self.parse_expr(false)?;
                    Ok(format!("(return {val})"))
                }
            }
            Tok::If => {
                self.pos += 1;
                self.skip_nl();
                self.expect(Tok::LParen)?;
                self.skip_nl();
                let cond = self.parse_expr(false)?;
                self.skip_nl();
                self.expect(Tok::RParen)?;
                self.skip_nl();
                let then_b = self.parse_braced_body("control")?;
                let saved = self.pos;
                self.skip_nl();
                if self.peek() == Tok::Else {
                    self.pos += 1;
                    self.skip_nl();
                    let else_b = self.parse_braced_body("control")?;
                    Ok(format!("(if {cond} {then_b} {else_b})"))
                } else {
                    self.pos = saved;
                    Ok(format!("(if {cond} {then_b})"))
                }
            }
            Tok::When => self.parse_when(),
            _ => Err(()),
        }
    }

    fn next_after_one_is_lbrace(&self) -> bool {
        let mut i = self.pos + 1;
        while matches!(self.tokens.get(i), Some(Tok::Nl)) {
            i += 1;
        }
        self.tokens.get(i) == Some(&Tok::LBrace)
    }

    fn parse_lambda(&mut self, is_move: bool) -> Result<String, ()> {
        self.expect(Tok::LBrace)?;
        self.skip_nl();
        let params = self.try_lambda_header();
        let mut elems = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            elems.push(self.parse_statement(false)?);
            let mut separated = false;
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
                separated = true;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            if !separated {
                return Err(());
            }
        }
        let head = if is_move { "move-lambda" } else { "lambda" };
        let header_s = match params {
            None => "no-header".to_owned(),
            Some(ps) if ps.is_empty() => "params".to_owned(),
            Some(ps) => format!("params {}", ps.join(" ")),
        };
        let body_s = if elems.is_empty() {
            "(lbody)".to_owned()
        } else {
            format!("(lbody {})", elems.join(" "))
        };
        Ok(format!("({head} ({header_s}) {body_s})"))
    }

    fn try_lambda_header(&mut self) -> Option<Vec<&'a str>> {
        let saved = self.pos;
        if self.peek() == Tok::Arrow {
            self.pos += 1;
            return Some(Vec::new());
        }
        let mut params = Vec::new();
        while let Tok::Ident(name) = self.peek() {
            params.push(name);
            self.pos += 1;
            self.skip_nl();
            if self.peek() == Tok::Comma {
                self.pos += 1;
                self.skip_nl();
                continue;
            }
            if self.peek() == Tok::Arrow {
                self.pos += 1;
                return Some(params);
            }
            break;
        }
        self.pos = saved;
        None
    }

    fn parse_when(&mut self) -> Result<String, ()> {
        self.expect(Tok::When)?;
        self.skip_nl();
        self.expect(Tok::LBrace)?;
        let mut entries = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            let conds = if self.peek() == Tok::Else {
                self.pos += 1;
                "else".to_owned()
            } else {
                let mut list = Vec::new();
                loop {
                    self.skip_nl();
                    list.push(self.parse_expr(false)?);
                    self.skip_nl();
                    if self.peek() == Tok::Comma {
                        self.pos += 1;
                        continue;
                    }
                    break;
                }
                format!("conds {}", list.join(" "))
            };
            self.skip_nl();
            self.expect(Tok::Arrow)?;
            self.skip_nl();
            let body = if self.peek() == Tok::LBrace {
                self.parse_braced_body("control")?
            } else {
                self.parse_expr(true)?
            };
            entries.push(format!("(entry ({conds}) {body})"));
            let mut separated = false;
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
                separated = true;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            if !separated {
                return Err(());
            }
        }
        Ok(format!("(when {})", entries.join(" ")))
    }
}

pub fn run_reference(source: &str) -> Outcome {
    let mut sources = SourceMap::new();
    let id = sources.add_source("ref.ko", source).unwrap();
    let lexed = lex(&sources, id).unwrap();
    let Some(tokens) = to_stream(&sources, &lexed) else {
        return Outcome::Rejected;
    };
    let mut parser = RefParser::new(&tokens);
    match parser.parse_root_block() {
        Ok(sexpr) => Outcome::Accepted(sexpr),
        Err(()) => Outcome::Rejected,
    }
}
