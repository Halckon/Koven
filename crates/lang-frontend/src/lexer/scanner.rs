use crate::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity, codes, ordered_diagnostics},
    source::{SourceId, SourceMap},
};

use super::{
    InvalidKind, Keyword, LexedFile, Lexeme, LexemeKind, LexerInternalError, ReservedWord, Symbol,
    TokenKind, TriviaKind,
};

#[derive(Clone, Copy)]
enum Mode {
    Normal,
    String { start: usize },
    Interpolation { start: usize, brace_depth: usize },
}

#[derive(Clone, Copy)]
enum LexicalError {
    UnexpectedCharacter,
    ReservedWord,
    UnterminatedBlockComment,
    UnterminatedString,
    UnterminatedInterpolation,
    InvalidStringEscape,
    InvalidCharLiteral,
    InvalidNumericLiteral,
}

impl LexicalError {
    const fn code(self) -> &'static str {
        match self {
            Self::UnexpectedCharacter => codes::UNEXPECTED_CHARACTER,
            Self::ReservedWord => codes::RESERVED_WORD,
            Self::UnterminatedBlockComment => codes::UNTERMINATED_BLOCK_COMMENT,
            Self::UnterminatedString => codes::UNTERMINATED_STRING,
            Self::UnterminatedInterpolation => codes::UNTERMINATED_INTERPOLATION,
            Self::InvalidStringEscape => codes::INVALID_STRING_ESCAPE,
            Self::InvalidCharLiteral => codes::INVALID_CHAR_LITERAL,
            Self::InvalidNumericLiteral => codes::INVALID_NUMERIC_LITERAL,
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::UnexpectedCharacter => "unexpected character",
            Self::ReservedWord => "reserved word is not available in Koven v1",
            Self::UnterminatedBlockComment => "unterminated block comment",
            Self::UnterminatedString => "unterminated string literal",
            Self::UnterminatedInterpolation => "unterminated string interpolation",
            Self::InvalidStringEscape => "invalid escape in string literal",
            Self::InvalidCharLiteral => "invalid character literal",
            Self::InvalidNumericLiteral => "invalid numeric literal",
        }
    }
}

pub(super) fn scan(
    sources: &SourceMap,
    source_id: SourceId,
) -> Result<LexedFile, LexerInternalError> {
    let text = sources.source_text(source_id)?;
    let catalog = codes::catalog()?;
    let mut scanner = Scanner {
        sources,
        source_id,
        text,
        catalog,
        offset: 0,
        modes: vec![Mode::Normal],
        lexemes: Vec::new(),
        diagnostics: Vec::new(),
        terminal_error_at_eof: false,
    };
    scanner.run()?;

    let diagnostics = ordered_diagnostics(sources, &scanner.diagnostics)?
        .into_iter()
        .cloned()
        .collect();
    Ok(LexedFile {
        source_id,
        lexemes: scanner.lexemes,
        diagnostics,
    })
}

struct Scanner<'source> {
    sources: &'source SourceMap,
    source_id: SourceId,
    text: &'source str,
    catalog: DiagnosticCodeCatalog,
    offset: usize,
    modes: Vec<Mode>,
    lexemes: Vec<Lexeme>,
    diagnostics: Vec<Diagnostic>,
    terminal_error_at_eof: bool,
}

impl Scanner<'_> {
    fn run(&mut self) -> Result<(), LexerInternalError> {
        while self.offset < self.text.len() {
            match self.current_mode() {
                Mode::Normal | Mode::Interpolation { .. } => self.scan_normal()?,
                Mode::String { .. } => self.scan_string()?,
            }
        }

        if !self.terminal_error_at_eof {
            match self.current_mode() {
                Mode::String { start } => {
                    self.add_diagnostic(LexicalError::UnterminatedString, start, self.offset)?;
                }
                Mode::Interpolation { start, .. } => {
                    self.add_diagnostic(
                        LexicalError::UnterminatedInterpolation,
                        start,
                        self.offset,
                    )?;
                }
                Mode::Normal => {}
            }
        }
        self.emit(LexemeKind::Eof, self.offset, self.offset)
    }

    fn current_mode(&self) -> Mode {
        self.modes.last().copied().unwrap_or(Mode::Normal)
    }

    fn scan_normal(&mut self) -> Result<(), LexerInternalError> {
        let start = self.offset;
        let rest = &self.text[start..];

        if rest.starts_with("\r\n") {
            self.offset += 2;
            return self.emit_trivia(TriviaKind::Newline, start);
        }
        if rest.starts_with('\n') {
            self.offset += 1;
            return self.emit_trivia(TriviaKind::Newline, start);
        }
        if rest.starts_with([' ', '\t']) {
            self.offset += rest
                .bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
            return self.emit_trivia(TriviaKind::Whitespace, start);
        }
        if rest.starts_with("//") {
            self.offset += rest.find(['\r', '\n']).unwrap_or(rest.len());
            return self.emit_trivia(TriviaKind::LineComment, start);
        }
        if let Some(comment_body) = rest.strip_prefix("/*") {
            if let Some(relative_end) = comment_body.find("*/") {
                self.offset += 2 + relative_end + 2;
                return self.emit_trivia(TriviaKind::BlockComment, start);
            }
            self.offset = self.text.len();
            self.terminal_error_at_eof = true;
            self.emit_invalid(InvalidKind::UnterminatedBlockComment, start)?;
            return self.add_diagnostic(LexicalError::UnterminatedBlockComment, start, self.offset);
        }

        if self.in_interpolation() && rest.starts_with('}') {
            let depth = self.interpolation_depth();
            self.offset += 1;
            if depth == 0 {
                self.modes.pop();
                return self.emit_token(TokenKind::InterpolationEnd, start);
            }
            self.set_interpolation_depth(depth - 1);
            return self.emit_symbol(Symbol::RightBrace, start);
        }
        if self.in_interpolation() && rest.starts_with('{') {
            let depth = self.interpolation_depth();
            self.set_interpolation_depth(depth + 1);
            self.offset += 1;
            return self.emit_symbol(Symbol::LeftBrace, start);
        }
        if rest.starts_with('"') {
            self.offset += 1;
            self.emit_token(TokenKind::StringStart, start)?;
            self.modes.push(Mode::String { start });
            return Ok(());
        }
        if rest.starts_with('\'') {
            return self.scan_char();
        }
        if rest.as_bytes()[0].is_ascii_digit() {
            return self.scan_number();
        }
        if is_identifier_start(rest.as_bytes()[0]) {
            return self.scan_identifier();
        }
        if let Some((kind, len)) = compound_word_symbol(rest) {
            self.offset += len;
            return self.emit_token(TokenKind::Symbol(kind), start);
        }
        if let Some((kind, len)) = fixed_symbol(rest) {
            self.offset += len;
            return self.emit_token(TokenKind::Symbol(kind), start);
        }

        let character =
            rest.chars()
                .next()
                .ok_or(crate::source::SourceError::OffsetOutOfBounds {
                    source_id: self.source_id,
                    offset: start,
                    source_len: self.text.len(),
                })?;
        self.offset += character.len_utf8();
        self.emit_invalid(InvalidKind::UnexpectedCharacter, start)?;
        self.add_diagnostic(LexicalError::UnexpectedCharacter, start, self.offset)
    }

    fn scan_identifier(&mut self) -> Result<(), LexerInternalError> {
        let start = self.offset;
        self.offset += self.text[start..]
            .bytes()
            .take_while(|byte| is_identifier_continue(*byte))
            .count();
        let word = &self.text[start..self.offset];

        if word == "as" && self.text[self.offset..].starts_with('?') {
            self.offset += 1;
            return self.emit_token(TokenKind::Symbol(Symbol::AsQuestion), start);
        }
        if let Some(keyword) = keyword(word) {
            return self.emit_token(TokenKind::Keyword(keyword), start);
        }
        if let Some(reserved) = reserved_word(word) {
            self.emit_token(TokenKind::ReservedWord(reserved), start)?;
            return self.add_diagnostic(LexicalError::ReservedWord, start, self.offset);
        }
        self.emit_token(TokenKind::Identifier, start)
    }

    fn scan_number(&mut self) -> Result<(), LexerInternalError> {
        let start = self.offset;
        self.consume_ascii_digits();
        let mut kind = TokenKind::IntegerLiteral;
        let rest = &self.text[self.offset..];
        if rest.starts_with('.') && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
            self.offset += 1;
            self.consume_ascii_digits();
            kind = TokenKind::FloatLiteral;
        }

        if self
            .text
            .as_bytes()
            .get(self.offset)
            .is_some_and(|byte| is_identifier_continue(*byte))
        {
            self.offset += self.text[self.offset..]
                .bytes()
                .take_while(|byte| is_identifier_continue(*byte))
                .count();
            self.emit_invalid(InvalidKind::InvalidNumericLiteral, start)?;
            return self.add_diagnostic(LexicalError::InvalidNumericLiteral, start, self.offset);
        }
        self.emit_token(kind, start)
    }

    fn consume_ascii_digits(&mut self) {
        self.offset += self.text[self.offset..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
    }

    fn scan_char(&mut self) -> Result<(), LexerInternalError> {
        let start = self.offset;
        self.offset += 1;
        let mut scalar_count = 0_usize;
        let mut valid = true;
        let mut closed = false;

        while self.offset < self.text.len() {
            let character = self.next_char()?;
            match character {
                '\'' => {
                    self.offset += 1;
                    closed = true;
                    break;
                }
                '\r' | '\n' => break,
                '\\' => {
                    self.offset += 1;
                    if self.offset == self.text.len() {
                        valid = false;
                        break;
                    }
                    let escaped = self.next_char()?;
                    if matches!(escaped, '\r' | '\n') {
                        valid = false;
                        break;
                    }
                    valid &= is_common_escape(escaped);
                    scalar_count += 1;
                    self.offset += escaped.len_utf8();
                }
                _ => {
                    scalar_count += 1;
                    self.offset += character.len_utf8();
                }
            }
        }

        if closed && valid && scalar_count == 1 {
            return self.emit_token(TokenKind::CharLiteral, start);
        }
        if !closed && self.offset == self.text.len() {
            self.terminal_error_at_eof = true;
        }
        self.emit_invalid(InvalidKind::InvalidCharLiteral, start)?;
        self.add_diagnostic(LexicalError::InvalidCharLiteral, start, self.offset)
    }

    fn scan_string(&mut self) -> Result<(), LexerInternalError> {
        let text_start = self.offset;
        while self.offset < self.text.len() {
            let rest = &self.text[self.offset..];
            if rest.starts_with('"') || rest.starts_with("${") || rest.starts_with(['\r', '\n']) {
                break;
            }
            let character = self.next_char()?;
            if character != '\\' {
                self.offset += character.len_utf8();
                continue;
            }

            let escape_start = self.offset;
            let after_slash = self.offset + 1;
            let Some(escaped) = self.text[after_slash..].chars().next() else {
                self.emit_string_text(text_start, escape_start)?;
                self.offset = after_slash;
                self.emit_invalid(InvalidKind::InvalidStringEscape, escape_start)?;
                self.add_diagnostic(LexicalError::InvalidStringEscape, escape_start, self.offset)?;
                self.modes.pop();
                self.terminal_error_at_eof = true;
                return Ok(());
            };
            if matches!(escaped, '\r' | '\n') {
                self.emit_string_text(text_start, escape_start)?;
                self.offset = after_slash;
                self.emit_invalid(InvalidKind::InvalidStringEscape, escape_start)?;
                self.add_diagnostic(LexicalError::InvalidStringEscape, escape_start, self.offset)?;
                self.modes.pop();
                return Ok(());
            }
            if is_string_escape(escaped) {
                self.offset = after_slash + escaped.len_utf8();
                continue;
            }

            self.emit_string_text(text_start, escape_start)?;
            self.offset = after_slash + escaped.len_utf8();
            self.emit_invalid(InvalidKind::InvalidStringEscape, escape_start)?;
            self.add_diagnostic(LexicalError::InvalidStringEscape, escape_start, self.offset)?;
            return Ok(());
        }

        if self.offset > text_start {
            return self.emit_token_range(TokenKind::StringText, text_start, self.offset);
        }
        let start = self.offset;
        let rest = &self.text[start..];
        if rest.starts_with('"') {
            self.offset += 1;
            self.modes.pop();
            return self.emit_token(TokenKind::StringEnd, start);
        }
        if rest.starts_with("${") {
            self.offset += 2;
            self.emit_token(TokenKind::InterpolationStart, start)?;
            self.modes.push(Mode::Interpolation {
                start,
                brace_depth: 0,
            });
            return Ok(());
        }
        if rest.starts_with(['\r', '\n']) {
            let string_start = match self.current_mode() {
                Mode::String { start } => start,
                _ => start,
            };
            self.add_diagnostic(LexicalError::UnterminatedString, string_start, self.offset)?;
            self.modes.pop();
        }
        Ok(())
    }

    fn emit_string_text(&mut self, start: usize, end: usize) -> Result<(), LexerInternalError> {
        if start == end {
            return Ok(());
        }
        self.emit_token_range(TokenKind::StringText, start, end)
    }

    fn next_char(&self) -> Result<char, LexerInternalError> {
        self.text[self.offset..].chars().next().ok_or_else(|| {
            crate::source::SourceError::OffsetOutOfBounds {
                source_id: self.source_id,
                offset: self.offset,
                source_len: self.text.len(),
            }
            .into()
        })
    }

    fn in_interpolation(&self) -> bool {
        matches!(self.current_mode(), Mode::Interpolation { .. })
    }

    fn interpolation_depth(&self) -> usize {
        match self.current_mode() {
            Mode::Interpolation { brace_depth, .. } => brace_depth,
            _ => 0,
        }
    }

    fn set_interpolation_depth(&mut self, depth: usize) {
        if let Some(Mode::Interpolation { brace_depth, .. }) = self.modes.last_mut() {
            *brace_depth = depth;
        }
    }

    fn emit_token(&mut self, kind: TokenKind, start: usize) -> Result<(), LexerInternalError> {
        self.emit_token_range(kind, start, self.offset)
    }

    fn emit_token_range(
        &mut self,
        kind: TokenKind,
        start: usize,
        end: usize,
    ) -> Result<(), LexerInternalError> {
        self.emit(LexemeKind::Token(kind), start, end)
    }

    fn emit_symbol(&mut self, kind: Symbol, start: usize) -> Result<(), LexerInternalError> {
        self.emit_token(TokenKind::Symbol(kind), start)
    }

    fn emit_trivia(&mut self, kind: TriviaKind, start: usize) -> Result<(), LexerInternalError> {
        self.emit(LexemeKind::Trivia(kind), start, self.offset)
    }

    fn emit_invalid(&mut self, kind: InvalidKind, start: usize) -> Result<(), LexerInternalError> {
        self.emit(LexemeKind::Invalid(kind), start, self.offset)
    }

    fn emit(
        &mut self,
        kind: LexemeKind,
        start: usize,
        end: usize,
    ) -> Result<(), LexerInternalError> {
        let span = self.sources.span(self.source_id, start, end)?;
        self.lexemes.push(Lexeme { kind, span });
        Ok(())
    }

    fn add_diagnostic(
        &mut self,
        error: LexicalError,
        start: usize,
        end: usize,
    ) -> Result<(), LexerInternalError> {
        let span = self.sources.span(self.source_id, start, end)?;
        let code = self.catalog.resolve(error.code())?;
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            error.message(),
            span,
        )?);
        Ok(())
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_common_escape(character: char) -> bool {
    matches!(character, '\\' | '\'' | '"' | 'n' | 'r' | 't' | '0')
}

fn is_string_escape(character: char) -> bool {
    is_common_escape(character) || character == '$'
}

fn compound_word_symbol(text: &str) -> Option<(Symbol, usize)> {
    for (word, kind) in [("!in", Symbol::BangIn), ("!is", Symbol::BangIs)] {
        if text.starts_with(word)
            && text
                .as_bytes()
                .get(word.len())
                .is_none_or(|byte| !is_identifier_continue(*byte))
        {
            return Some((kind, word.len()));
        }
    }
    None
}

fn fixed_symbol(text: &str) -> Option<(Symbol, usize)> {
    const SYMBOLS: &[(&str, Symbol)] = &[
        ("..<", Symbol::DotDotLess),
        ("?.", Symbol::QuestionDot),
        ("?:", Symbol::QuestionColon),
        ("!!", Symbol::BangBang),
        ("::", Symbol::ColonColon),
        ("->", Symbol::Arrow),
        ("..", Symbol::DotDot),
        ("<=", Symbol::LessEqual),
        (">=", Symbol::GreaterEqual),
        ("==", Symbol::EqualEqual),
        ("!=", Symbol::BangEqual),
        ("&&", Symbol::AndAnd),
        ("||", Symbol::OrOr),
        ("+=", Symbol::PlusEqual),
        ("-=", Symbol::MinusEqual),
        ("*=", Symbol::StarEqual),
        ("/=", Symbol::SlashEqual),
        ("%=", Symbol::PercentEqual),
        ("(", Symbol::LeftParen),
        (")", Symbol::RightParen),
        ("[", Symbol::LeftBracket),
        ("]", Symbol::RightBracket),
        ("{", Symbol::LeftBrace),
        ("}", Symbol::RightBrace),
        (",", Symbol::Comma),
        (":", Symbol::Colon),
        (";", Symbol::Semicolon),
        ("@", Symbol::At),
        (".", Symbol::Dot),
        ("?", Symbol::Question),
        ("!", Symbol::Bang),
        ("*", Symbol::Star),
        ("/", Symbol::Slash),
        ("%", Symbol::Percent),
        ("+", Symbol::Plus),
        ("-", Symbol::Minus),
        ("<", Symbol::Less),
        (">", Symbol::Greater),
        ("&", Symbol::Ampersand),
        ("=", Symbol::Equal),
    ];
    SYMBOLS
        .iter()
        .find(|(spelling, _)| text.starts_with(spelling))
        .map(|(spelling, kind)| (*kind, spelling.len()))
}

fn keyword(word: &str) -> Option<Keyword> {
    Some(match word {
        "class" => Keyword::Class,
        "companion" => Keyword::Companion,
        "const" => Keyword::Const,
        "enum" => Keyword::Enum,
        "extern" => Keyword::Extern,
        "fun" => Keyword::Fun,
        "import" => Keyword::Import,
        "interface" => Keyword::Interface,
        "module" => Keyword::Module,
        "object" => Keyword::Object,
        "typealias" => Keyword::Typealias,
        "val" => Keyword::Val,
        "value" => Keyword::Value,
        "var" => Keyword::Var,
        "vararg" => Keyword::Vararg,
        "break" => Keyword::Break,
        "continue" => Keyword::Continue,
        "else" => Keyword::Else,
        "for" => Keyword::For,
        "if" => Keyword::If,
        "in" => Keyword::In,
        "is" => Keyword::Is,
        "loop" => Keyword::Loop,
        "return" => Keyword::Return,
        "when" => Keyword::When,
        "while" => Keyword::While,
        "borrow" => Keyword::Borrow,
        "inout" => Keyword::Inout,
        "move" => Keyword::Move,
        "own" => Keyword::Own,
        "unsafe" => Keyword::Unsafe,
        "internal" => Keyword::Internal,
        "private" => Keyword::Private,
        "public" => Keyword::Public,
        "as" => Keyword::As,
        "false" => Keyword::False,
        "null" => Keyword::Null,
        "operator" => Keyword::Operator,
        "override" => Keyword::Override,
        "super" => Keyword::Super,
        "this" => Keyword::This,
        "true" => Keyword::True,
        _ => return None,
    })
}

fn reserved_word(word: &str) -> Option<ReservedWord> {
    Some(match word {
        "async" => ReservedWord::Async,
        "await" => ReservedWord::Await,
        "suspend" => ReservedWord::Suspend,
        "actor" => ReservedWord::Actor,
        "spawn" => ReservedWord::Spawn,
        "sealed" => ReservedWord::Sealed,
        "dyn" => ReservedWord::Dyn,
        "where" => ReservedWord::Where,
        "yield" => ReservedWord::Yield,
        "macro" => ReservedWord::Macro,
        "reify" => ReservedWord::Reify,
        _ => return None,
    })
}
