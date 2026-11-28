//! Independent Guide v0.47 (Chapters 02 / 05 / 08) reference parser for F3e
//! file headers, declarations, class-family headers, and member modifiers.
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
    Package,
    Import,
    As,
    Val,
    Var,
    Const,
    Fun,
    Class,
    Interface,
    Enum,
    Object,
    Companion,
    Public,
    Internal,
    Private,
    Override,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Lt,
    Gt,
    Comma,
    Colon,
    Semi,
    Dot,
    Star,
    Eq,
    Plus,
    Question,
    Nl,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerKind {
    TopLevel,
    Class,
    ValueClass,
    Interface,
    EnumClass,
    Object,
    Companion,
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
            LexemeKind::Token(TokenKind::Keyword(Keyword::Package)) => Tok::Package,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Import)) => Tok::Import,
            LexemeKind::Token(TokenKind::Keyword(Keyword::As)) => Tok::As,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Val)) => Tok::Val,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Var)) => Tok::Var,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Const)) => Tok::Const,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Fun)) => Tok::Fun,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Class)) => Tok::Class,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Interface)) => Tok::Interface,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Enum)) => Tok::Enum,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Object)) => Tok::Object,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Companion)) => Tok::Companion,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Public)) => Tok::Public,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Internal)) => Tok::Internal,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Private)) => Tok::Private,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Override)) => Tok::Override,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => Tok::LParen,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => Tok::RParen,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => Tok::LBrace,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => Tok::RBrace,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => Tok::Lt,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => Tok::Gt,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => Tok::Comma,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Colon)) => Tok::Colon,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) => Tok::Semi,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot)) => Tok::Dot,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Star)) => Tok::Star,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => Tok::Eq,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => Tok::Plus,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Question)) => Tok::Question,
            other => panic!("unexpected token in F3e subset: {other:?} ({spelling:?})"),
        };
        out.push(tok);
    }
    Some(out)
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

    fn expect_ident(&mut self) -> Result<&'a str, ()> {
        match self.peek() {
            Tok::Ident(name) => {
                self.pos += 1;
                Ok(name)
            }
            _ => Err(()),
        }
    }

    /// Consumes a mandatory separator between top-level constructs (`NL+` or `;` + `NL*`),
    /// forbidding double `;;`. Returns `(saw_sep, hit_eof)`.
    fn consume_file_separator(&mut self) -> Result<(bool, bool), ()> {
        let mut saw_nl = self.skip_nl();
        let mut saw_semi = false;
        if self.peek() == Tok::Semi {
            self.pos += 1;
            saw_semi = true;
            saw_nl |= self.skip_nl();
            if self.peek() == Tok::Semi {
                return Err(());
            }
        }
        let hit_eof = self.peek() == Tok::Eof;
        Ok((saw_nl || saw_semi, hit_eof))
    }

    fn parse_file(&mut self) -> Result<String, ()> {
        self.skip_nl();
        if self.peek() == Tok::Semi {
            return Err(());
        }
        if self.peek() == Tok::Eof {
            return Ok("(file)".to_owned());
        }

        let mut parts = Vec::new();
        if self.peek() == Tok::Package {
            parts.push(self.parse_package()?);
            let (sep, eof) = self.consume_file_separator()?;
            if eof {
                return Ok(format!("(file {})", parts.join(" ")));
            }
            if !sep {
                return Err(());
            }
        }

        while self.peek() == Tok::Import {
            parts.push(self.parse_import()?);
            let (sep, eof) = self.consume_file_separator()?;
            if eof {
                return Ok(format!("(file {})", parts.join(" ")));
            }
            if !sep {
                return Err(());
            }
        }

        while self.peek() != Tok::Eof {
            if matches!(self.peek(), Tok::Package | Tok::Import | Tok::Semi) {
                return Err(());
            }
            parts.push(self.parse_item(OwnerKind::TopLevel)?);
            let (sep, eof) = self.consume_file_separator()?;
            if eof {
                break;
            }
            if !sep {
                return Err(());
            }
        }

        self.expect(Tok::Eof)?;
        Ok(format!("(file {})", parts.join(" ")))
    }

    fn parse_standalone_declaration(&mut self) -> Result<String, ()> {
        self.skip_nl();
        let item = self.parse_item(OwnerKind::TopLevel)?;
        self.skip_nl();
        self.expect(Tok::Eof)?;
        Ok(item)
    }

    fn parse_package(&mut self) -> Result<String, ()> {
        self.expect(Tok::Package)?;
        self.skip_nl();
        let mut segs = vec![self.expect_ident()?];
        while self.peek() == Tok::Dot {
            self.pos += 1;
            segs.push(self.expect_ident()?);
        }
        Ok(format!("(package {})", segs.join(".")))
    }

    fn parse_import(&mut self) -> Result<String, ()> {
        self.expect(Tok::Import)?;
        self.skip_nl();
        let mut segs = vec![self.expect_ident()?];
        let mut wildcard = false;
        while self.peek() == Tok::Dot {
            self.pos += 1;
            if self.peek() == Tok::Star {
                self.pos += 1;
                wildcard = true;
                break;
            }
            segs.push(self.expect_ident()?);
        }
        let mut alias = None;
        if self.peek() == Tok::As {
            if wildcard {
                return Err(());
            }
            self.pos += 1;
            alias = Some(self.expect_ident()?);
        }
        let mut out = format!("(import {}", segs.join("."));
        if wildcard {
            out.push_str(".*");
        }
        if let Some(a) = alias {
            out.push_str(&format!(" as {a}"));
        }
        out.push(')');
        Ok(out)
    }

    fn parse_visibility(&mut self) -> Option<&'static str> {
        let vis = match self.peek() {
            Tok::Public => "public",
            Tok::Internal => "internal",
            Tok::Private => "private",
            _ => return None,
        };
        self.pos += 1;
        Some(vis)
    }

    fn parse_item(&mut self, owner: OwnerKind) -> Result<String, ()> {
        let vis = self.parse_visibility();
        if vis.is_some() {
            self.skip_nl();
        }
        let has_override = if self.peek() == Tok::Override {
            self.pos += 1;
            self.skip_nl();
            true
        } else {
            false
        };
        let receiver_mode = match self.peek() {
            Tok::Ident(m @ ("own" | "borrow" | "inout"))
                if matches!(self.tokens.get(self.pos + 1), Some(Tok::Fun)) =>
            {
                self.pos += 1;
                Some(m)
            }
            _ => None,
        };

        // `deinit() { ... }` only in ordinary `class` with no modifiers.
        if matches!(self.peek(), Tok::Ident("deinit")) {
            if owner != OwnerKind::Class || vis.is_some() || has_override || receiver_mode.is_some()
            {
                return Err(());
            }
            self.pos += 1;
            self.expect(Tok::LParen)?;
            self.skip_nl();
            self.expect(Tok::RParen)?;
            self.skip_nl();
            let body = self.parse_simple_block()?;
            return Ok(format!("(deinit {body})"));
        }

        // `companion { ... }` only in class/value class/interface/enum class; visibility allowed.
        if self.peek() == Tok::Companion {
            if !matches!(
                owner,
                OwnerKind::Class
                    | OwnerKind::ValueClass
                    | OwnerKind::Interface
                    | OwnerKind::EnumClass
            ) || has_override
                || receiver_mode.is_some()
            {
                return Err(());
            }
            self.pos += 1;
            self.skip_nl();
            let members = self.parse_member_body(OwnerKind::Companion)?;
            return Ok(wrap_mods(
                vis,
                false,
                None,
                format!("(companion {members})"),
            ));
        }

        // Class-family declarations (top-level only in this slice; no override/receiver mode).
        if matches!(
            self.peek(),
            Tok::Class | Tok::Interface | Tok::Enum | Tok::Object
        ) || (matches!(self.peek(), Tok::Ident("value"))
            && matches!(self.tokens.get(self.pos + 1), Some(Tok::Class)))
        {
            if owner != OwnerKind::TopLevel || has_override || receiver_mode.is_some() {
                return Err(());
            }
            let decl = self.parse_classifier()?;
            return Ok(wrap_mods(vis, false, None, decl));
        }

        // `const val`
        if self.peek() == Tok::Const {
            if has_override || receiver_mode.is_some() {
                return Err(());
            }
            if !matches!(
                owner,
                OwnerKind::TopLevel | OwnerKind::Object | OwnerKind::Companion
            ) {
                return Err(());
            }
            self.pos += 1;
            self.skip_nl();
            self.expect(Tok::Val)?;
            self.skip_nl();
            let name = self.expect_ident()?;
            let ty = self.parse_optional_type_annotation()?;
            self.skip_nl();
            self.expect(Tok::Eq)?;
            self.skip_nl();
            let init = self.parse_simple_expr()?;
            let inner = match ty {
                Some(t) => format!("(const-val {name} : {t} = {init})"),
                None => format!("(const-val {name} = {init})"),
            };
            return Ok(wrap_mods(vis, false, None, inner));
        }

        // `val` / `var`
        if matches!(self.peek(), Tok::Val | Tok::Var) {
            if owner != OwnerKind::TopLevel || has_override || receiver_mode.is_some() {
                return Err(());
            }
            let kind = if self.peek() == Tok::Val {
                "val"
            } else {
                "var"
            };
            self.pos += 1;
            self.skip_nl();
            let name = self.expect_ident()?;
            let ty = self.parse_optional_type_annotation()?;
            self.skip_nl();
            self.expect(Tok::Eq)?;
            self.skip_nl();
            let init = self.parse_simple_expr()?;
            let inner = match ty {
                Some(t) => format!("({kind} {name} : {t} = {init})"),
                None => format!("({kind} {name} = {init})"),
            };
            return Ok(wrap_mods(vis, false, None, inner));
        }

        // `fun`
        if self.peek() == Tok::Fun {
            if owner == OwnerKind::TopLevel && (has_override || receiver_mode.is_some()) {
                return Err(());
            }
            if owner == OwnerKind::Object && has_override {
                return Err(());
            }
            if owner == OwnerKind::Companion && (has_override || receiver_mode.is_some()) {
                return Err(());
            }
            if owner == OwnerKind::Interface
                && (matches!(vis, Some("internal" | "private")) || has_override)
            {
                return Err(());
            }
            let decl = self.parse_function()?;
            return Ok(wrap_mods(vis, has_override, receiver_mode, decl));
        }

        Err(())
    }

    fn parse_optional_type_annotation(&mut self) -> Result<Option<String>, ()> {
        if self.peek() == Tok::Colon {
            self.pos += 1;
            self.skip_nl();
            Ok(Some(self.parse_type_ref()?))
        } else {
            Ok(None)
        }
    }

    fn parse_type_ref(&mut self) -> Result<String, ()> {
        let mut out = self.expect_ident()?.to_owned();
        if self.peek() == Tok::Lt {
            self.pos += 1;
            let mut args = Vec::new();
            loop {
                self.skip_nl();
                args.push(self.parse_type_ref()?);
                self.skip_nl();
                if self.peek() == Tok::Comma {
                    self.pos += 1;
                    continue;
                }
                self.expect(Tok::Gt)?;
                break;
            }
            out = format!("{out}<{}>", args.join(","));
        }
        if self.peek() == Tok::Question {
            self.pos += 1;
            out.push('?');
        }
        Ok(out)
    }

    fn parse_function(&mut self) -> Result<String, ()> {
        self.expect(Tok::Fun)?;
        self.skip_nl();
        let tparams = if self.peek() == Tok::Lt {
            self.pos += 1;
            let mut list = Vec::new();
            loop {
                self.skip_nl();
                let name = self.expect_ident()?;
                let bound = if self.peek() == Tok::Colon {
                    self.pos += 1;
                    self.skip_nl();
                    Some(self.parse_type_ref()?)
                } else {
                    None
                };
                list.push(match bound {
                    Some(b) => format!("{name}:{b}"),
                    None => name.to_owned(),
                });
                self.skip_nl();
                if self.peek() == Tok::Comma {
                    self.pos += 1;
                    continue;
                }
                self.expect(Tok::Gt)?;
                break;
            }
            Some(format!("<{}>", list.join(",")))
        } else {
            None
        };

        self.skip_nl();
        let name = self.expect_ident()?;
        self.expect(Tok::LParen)?;
        let mut params = Vec::new();
        self.skip_nl();
        if self.peek() != Tok::RParen {
            loop {
                self.skip_nl();
                let mode = match self.peek() {
                    Tok::Ident(m @ ("own" | "borrow" | "inout"))
                        if matches!(self.tokens.get(self.pos + 1), Some(Tok::Ident(_))) =>
                    {
                        self.pos += 1;
                        Some(m)
                    }
                    _ => None,
                };
                let pname = self.expect_ident()?;
                self.expect(Tok::Colon)?;
                self.skip_nl();
                let pty = self.parse_type_ref()?;
                params.push(match mode {
                    Some(m) => format!("{m} {pname}:{pty}"),
                    None => format!("{pname}:{pty}"),
                });
                self.skip_nl();
                if self.peek() == Tok::Comma {
                    self.pos += 1;
                    continue;
                }
                break;
            }
        }
        self.expect(Tok::RParen)?;

        let ret_ty = self.parse_optional_type_annotation()?;
        let body = match self.peek() {
            Tok::Eq => {
                // `= expr` requires an explicit `: Type` annotation!
                if ret_ty.is_none() {
                    return Err(());
                }
                self.pos += 1;
                self.skip_nl();
                format!("= {}", self.parse_simple_expr()?)
            }
            Tok::LBrace => self.parse_simple_block()?,
            _ => "absent".to_owned(),
        };

        let mut head = format!("fun {name}");
        if let Some(tp) = tparams {
            head.push_str(&tp);
        }
        head.push_str(&format!("({})", params.join(", ")));
        if let Some(rt) = ret_ty {
            head.push_str(&format!(" : {rt}"));
        }
        Ok(format!("({head} {body})"))
    }

    fn parse_classifier(&mut self) -> Result<String, ()> {
        let (kind_tag, owner_kind) = match self.peek() {
            Tok::Ident("value") => {
                self.pos += 1;
                self.expect(Tok::Class)?;
                ("value-class", OwnerKind::ValueClass)
            }
            Tok::Class => {
                self.pos += 1;
                ("class", OwnerKind::Class)
            }
            Tok::Interface => {
                self.pos += 1;
                ("interface", OwnerKind::Interface)
            }
            Tok::Enum => {
                self.pos += 1;
                self.expect(Tok::Class)?;
                ("enum-class", OwnerKind::EnumClass)
            }
            Tok::Object => {
                self.pos += 1;
                ("object", OwnerKind::Object)
            }
            _ => return Err(()),
        };
        self.skip_nl();
        let name = self.expect_ident()?;

        let ctor = if self.peek() == Tok::LParen {
            if !matches!(owner_kind, OwnerKind::Class | OwnerKind::ValueClass) {
                return Err(());
            }
            let fields = self.parse_primary_constructor(owner_kind == OwnerKind::ValueClass)?;
            Some(format!("(ctor {})", fields.join(", ")))
        } else {
            if owner_kind == OwnerKind::ValueClass {
                return Err(());
            }
            None
        };

        let supers = if self.peek() == Tok::Colon {
            self.pos += 1;
            let mut list = Vec::new();
            loop {
                self.skip_nl();
                let ty = self.parse_type_ref()?;
                let del = if matches!(self.peek(), Tok::Ident("by")) {
                    if owner_kind != OwnerKind::Class {
                        return Err(());
                    }
                    self.pos += 1;
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                list.push(match del {
                    Some(target) => format!("{ty} by {target}"),
                    None => ty,
                });
                let saved = self.pos;
                self.skip_nl();
                if self.peek() == Tok::Comma {
                    self.pos += 1;
                    continue;
                }
                self.pos = saved;
                break;
            }
            Some(format!("(supers {})", list.join(", ")))
        } else {
            None
        };

        let body = if self.peek() == Tok::LBrace {
            if owner_kind == OwnerKind::EnumClass {
                Some(self.parse_enum_body()?)
            } else {
                Some(self.parse_member_body(owner_kind)?)
            }
        } else {
            if owner_kind == OwnerKind::EnumClass {
                return Err(());
            }
            None
        };

        let mut parts = vec![format!("{kind_tag} {name}")];
        if let Some(c) = ctor {
            parts.push(c);
        }
        if let Some(s) = supers {
            parts.push(s);
        }
        if let Some(b) = body {
            parts.push(b);
        }
        Ok(format!("({})", parts.join(" ")))
    }

    fn parse_primary_constructor(&mut self, require_non_empty: bool) -> Result<Vec<String>, ()> {
        self.expect(Tok::LParen)?;
        let mut fields = Vec::new();
        self.skip_nl();
        if self.peek() == Tok::RParen {
            if require_non_empty {
                return Err(());
            }
            self.pos += 1;
            return Ok(fields);
        }
        loop {
            self.skip_nl();
            let vis = self.parse_visibility();
            let kind = match self.peek() {
                Tok::Val => "val",
                Tok::Var => "var",
                _ => return Err(()),
            };
            self.pos += 1;
            let name = self.expect_ident()?;
            self.expect(Tok::Colon)?;
            self.skip_nl();
            let ty = self.parse_type_ref()?;
            fields.push(match vis {
                Some(v) => format!("{v} {kind} {name}:{ty}"),
                None => format!("{kind} {name}:{ty}"),
            });
            self.skip_nl();
            if self.peek() == Tok::Comma {
                self.pos += 1;
                continue;
            }
            self.expect(Tok::RParen)?;
            break;
        }
        Ok(fields)
    }

    fn parse_member_body(&mut self, owner: OwnerKind) -> Result<String, ()> {
        self.expect(Tok::LBrace)?;
        let mut members = Vec::new();
        loop {
            self.skip_nl();
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            if self.peek() == Tok::Semi {
                return Err(());
            }
            members.push(self.parse_item(owner)?);
            let mut sep = self.skip_nl();
            if self.peek() == Tok::Semi {
                self.pos += 1;
                sep = true;
                self.skip_nl();
                if self.peek() == Tok::Semi {
                    return Err(());
                }
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            if !sep {
                return Err(());
            }
        }
        if members.is_empty() {
            Ok("(body)".to_owned())
        } else {
            Ok(format!("(body {})", members.join(" ")))
        }
    }

    fn parse_enum_body(&mut self) -> Result<String, ()> {
        self.expect(Tok::LBrace)?;
        self.skip_nl();
        let mut variants = Vec::new();
        loop {
            let vname = self.expect_ident()?;
            let mut vparams = Vec::new();
            if self.peek() == Tok::LParen {
                self.pos += 1;
                self.skip_nl();
                if self.peek() == Tok::RParen {
                    return Err(());
                }
                loop {
                    self.skip_nl();
                    let pname = self.expect_ident()?;
                    self.expect(Tok::Colon)?;
                    self.skip_nl();
                    let pty = self.parse_type_ref()?;
                    vparams.push(format!("{pname}:{pty}"));
                    self.skip_nl();
                    if self.peek() == Tok::Comma {
                        self.pos += 1;
                        continue;
                    }
                    self.expect(Tok::RParen)?;
                    break;
                }
            }
            variants.push(if vparams.is_empty() {
                vname.to_owned()
            } else {
                format!("{vname}({})", vparams.join(", "))
            });
            self.skip_nl();
            if self.peek() == Tok::Comma {
                self.pos += 1;
                self.skip_nl();
                continue;
            }
            break;
        }
        let mut members = Vec::new();
        if self.peek() == Tok::Semi {
            self.pos += 1;
            self.skip_nl();
            // After `;` in enum body, at least one member is required!
            if self.peek() == Tok::RBrace {
                return Err(());
            }
            loop {
                members.push(self.parse_item(OwnerKind::EnumClass)?);
                let mut sep = self.skip_nl();
                if self.peek() == Tok::Semi {
                    self.pos += 1;
                    sep = true;
                    self.skip_nl();
                }
                if self.peek() == Tok::RBrace {
                    self.pos += 1;
                    break;
                }
                if !sep {
                    return Err(());
                }
            }
        } else {
            self.expect(Tok::RBrace)?;
        }
        let mut out = format!("(enum-body (variants {})", variants.join(", "));
        if !members.is_empty() {
            out.push_str(&format!(" (members {})", members.join(" ")));
        }
        out.push(')');
        Ok(out)
    }

    fn parse_simple_block(&mut self) -> Result<String, ()> {
        self.expect(Tok::LBrace)?;
        let mut elems = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            elems.push(self.parse_simple_expr()?);
            let mut sep = false;
            while matches!(self.peek(), Tok::Nl | Tok::Semi) {
                self.pos += 1;
                sep = true;
            }
            if self.peek() == Tok::RBrace {
                self.pos += 1;
                break;
            }
            if !sep {
                return Err(());
            }
        }
        if elems.is_empty() {
            Ok("(block)".to_owned())
        } else {
            Ok(format!("(block {})", elems.join(" ")))
        }
    }

    fn parse_simple_expr(&mut self) -> Result<String, ()> {
        let mut lhs = self.parse_atom()?;
        while self.peek() == Tok::Plus {
            self.pos += 1;
            self.skip_nl();
            let rhs = self.parse_atom()?;
            lhs = format!("(add {lhs} {rhs})");
        }
        Ok(lhs)
    }

    fn parse_atom(&mut self) -> Result<String, ()> {
        match self.peek() {
            Tok::Ident(name) => {
                self.pos += 1;
                Ok(format!("(id {name})"))
            }
            Tok::Int(lit) => {
                self.pos += 1;
                Ok(format!("(int {lit})"))
            }
            _ => Err(()),
        }
    }
}

fn wrap_mods(
    vis: Option<&'static str>,
    has_override: bool,
    receiver_mode: Option<&str>,
    inner: String,
) -> String {
    let mut mods = Vec::new();
    if let Some(v) = vis {
        mods.push(v);
    }
    if has_override {
        mods.push("override");
    }
    if let Some(m) = receiver_mode {
        mods.push(m);
    }
    if mods.is_empty() {
        inner
    } else {
        format!("(mod [{}] {inner})", mods.join(" "))
    }
}

pub fn run_file_reference(source: &str) -> Outcome {
    let mut sources = SourceMap::new();
    let id = sources.add_source("ref.ko", source).unwrap();
    let lexed = lex(&sources, id).unwrap();
    let Some(tokens) = to_stream(&sources, &lexed) else {
        return Outcome::Rejected;
    };
    let mut parser = RefParser::new(&tokens);
    match parser.parse_file() {
        Ok(sexpr) => Outcome::Accepted(sexpr),
        Err(()) => Outcome::Rejected,
    }
}

pub fn run_declaration_reference(source: &str) -> Outcome {
    let mut sources = SourceMap::new();
    let id = sources.add_source("ref-decl.ko", source).unwrap();
    let lexed = lex(&sources, id).unwrap();
    let Some(tokens) = to_stream(&sources, &lexed) else {
        return Outcome::Rejected;
    };
    let mut parser = RefParser::new(&tokens);
    match parser.parse_standalone_declaration() {
        Ok(sexpr) => Outcome::Accepted(sexpr),
        Err(()) => Outcome::Rejected,
    }
}
