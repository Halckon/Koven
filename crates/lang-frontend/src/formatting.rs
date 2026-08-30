//! 保留 token、注释与换行边界的保守源码格式化。

use std::{error::Error, fmt};

use crate::{
    diagnostic::Diagnostic,
    lexer::{Keyword, Lexeme, LexemeKind, LexerInternalError, Symbol, TokenKind, TriviaKind, lex},
    parser::{ParserInternalError, parse_file},
    source::{SourceError, SourceId, SourceMap},
};

/// 格式化失败的结构化边界。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormattingError {
    /// Source identity 或源码范围不属于调用方提供的 map。
    Source(SourceError),
    /// Lexer 内部边界失败。
    Lexer(LexerInternalError),
    /// Parser 内部边界失败。
    Parser(ParserInternalError),
    /// 用户源码包含词法或语法诊断，因此未产生部分格式化文本。
    Diagnostics(Vec<Diagnostic>),
    /// 零诊断产物违反了 formatter 依赖的 delimiter 不变量。
    InvalidDelimiterStream,
}

impl fmt::Display for FormattingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "formatter source error: {error}"),
            Self::Lexer(error) => write!(formatter, "formatter lexer error: {error}"),
            Self::Parser(error) => write!(formatter, "formatter parser error: {error}"),
            Self::Diagnostics(diagnostics) => write!(
                formatter,
                "source contains {} formatting-blocking diagnostic(s)",
                diagnostics.len()
            ),
            Self::InvalidDelimiterStream => {
                formatter.write_str("formatter received an invalid delimiter stream")
            }
        }
    }
}

impl Error for FormattingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Lexer(error) => Some(error),
            Self::Parser(error) => Some(error),
            Self::Diagnostics(_) | Self::InvalidDelimiterStream => None,
        }
    }
}

/// 格式化一份已经加载的合法 Koven 完整文件。
///
/// 非 trivia token、comment 和每个 newline lexeme 的原始字节保持不变；只规范水平空白和
/// delimiter 驱动的四空格缩进。词法或语法诊断会整体返回，不产生部分结果。
///
/// # Errors
///
/// source identity 无效、Lexer/Parser 内部失败、源码含诊断或零诊断 lexeme 违反 delimiter
/// 不变量时返回对应 [`FormattingError`]。
pub fn format_source(sources: &SourceMap, source_id: SourceId) -> Result<String, FormattingError> {
    let source = sources
        .source_text(source_id)
        .map_err(FormattingError::Source)?;
    let lexed = lex(sources, source_id).map_err(FormattingError::Lexer)?;
    if !lexed.diagnostics().is_empty() {
        return Err(FormattingError::Diagnostics(lexed.diagnostics().to_vec()));
    }

    let parsed = parse_file(sources, &lexed).map_err(FormattingError::Parser)?;
    if !parsed.diagnostics().is_empty() {
        return Err(FormattingError::Diagnostics(parsed.diagnostics().to_vec()));
    }

    Formatter::new(sources, source.len(), lexed.lexemes()).format()
}

struct Formatter<'a> {
    sources: &'a SourceMap,
    lexemes: &'a [Lexeme],
    previous_tokens: Vec<Option<TokenKind>>,
    output: String,
    delimiters: Vec<Symbol>,
    previous_content: Option<Content>,
    line_has_content: bool,
    pending_horizontal_space: bool,
}

impl<'a> Formatter<'a> {
    fn new(sources: &'a SourceMap, source_len: usize, lexemes: &'a [Lexeme]) -> Self {
        Self {
            sources,
            lexemes,
            previous_tokens: previous_tokens(lexemes),
            output: String::with_capacity(source_len),
            delimiters: Vec::new(),
            previous_content: None,
            line_has_content: false,
            pending_horizontal_space: false,
        }
    }

    fn format(mut self) -> Result<String, FormattingError> {
        for (index, lexeme) in self.lexemes.iter().copied().enumerate() {
            match lexeme.kind() {
                LexemeKind::Trivia(TriviaKind::Whitespace) => {
                    self.pending_horizontal_space = true;
                }
                LexemeKind::Trivia(TriviaKind::Newline) => self.write_newline(lexeme)?,
                LexemeKind::Trivia(TriviaKind::LineComment | TriviaKind::BlockComment) => {
                    self.write_comment(lexeme)?;
                }
                LexemeKind::Token(kind) => self.write_token(index, lexeme, kind)?,
                LexemeKind::Eof => self.pending_horizontal_space = false,
                LexemeKind::Invalid(_) => {
                    return Err(FormattingError::InvalidDelimiterStream);
                }
            }
        }

        if !self.delimiters.is_empty() {
            return Err(FormattingError::InvalidDelimiterStream);
        }
        Ok(self.output)
    }

    fn write_newline(&mut self, lexeme: Lexeme) -> Result<(), FormattingError> {
        self.pending_horizontal_space = false;
        self.output.push_str(self.slice(lexeme)?);
        self.previous_content = None;
        self.line_has_content = false;
        Ok(())
    }

    fn write_comment(&mut self, lexeme: Lexeme) -> Result<(), FormattingError> {
        if !self.line_has_content {
            self.write_indent(self.delimiters.len());
        } else if self.space_before_comment() {
            self.output.push(' ');
        }

        self.output.push_str(self.slice(lexeme)?);
        self.previous_content = Some(Content::Comment);
        self.line_has_content = true;
        self.pending_horizontal_space = false;
        Ok(())
    }

    fn write_token(
        &mut self,
        index: usize,
        lexeme: Lexeme,
        kind: TokenKind,
    ) -> Result<(), FormattingError> {
        if !self.line_has_content {
            self.write_indent(self.indent_for(kind));
        } else if self.space_between(index, kind) {
            self.output.push(' ');
        }

        self.output.push_str(self.slice(lexeme)?);
        self.update_delimiters(kind)?;
        self.previous_content = Some(Content::Token { index, kind });
        self.line_has_content = true;
        self.pending_horizontal_space = false;
        Ok(())
    }

    fn slice(&self, lexeme: Lexeme) -> Result<&'a str, FormattingError> {
        self.sources
            .slice(lexeme.span())
            .map_err(FormattingError::Source)
    }

    fn write_indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.output.push_str("    ");
        }
    }

    fn indent_for(&self, kind: TokenKind) -> usize {
        let Some(closing) = closing_symbol(kind) else {
            return self.delimiters.len();
        };
        if self
            .delimiters
            .last()
            .is_some_and(|opening| delimiters_match(*opening, closing))
        {
            self.delimiters.len().saturating_sub(1)
        } else {
            self.delimiters.len()
        }
    }

    fn update_delimiters(&mut self, kind: TokenKind) -> Result<(), FormattingError> {
        let TokenKind::Symbol(symbol) = kind else {
            return Ok(());
        };
        if is_opening(symbol) {
            self.delimiters.push(symbol);
            return Ok(());
        }
        if is_closing(symbol) {
            let Some(opening) = self.delimiters.pop() else {
                return Err(FormattingError::InvalidDelimiterStream);
            };
            if !delimiters_match(opening, symbol) {
                return Err(FormattingError::InvalidDelimiterStream);
            }
        }
        Ok(())
    }

    fn space_before_comment(&self) -> bool {
        match self.previous_content {
            Some(Content::Token { kind, .. }) => !suppresses_space_after(kind),
            Some(Content::Comment) => true,
            None => false,
        }
    }

    fn space_between(&self, current_index: usize, current: TokenKind) -> bool {
        let Some(previous) = self.previous_content else {
            return false;
        };
        let Content::Token {
            index: previous_index,
            kind: previous_kind,
        } = previous
        else {
            return !suppresses_space_before(current);
        };

        if is_string_join(previous_kind, current)
            || suppresses_space_after(previous_kind)
            || suppresses_space_before(current)
            || is_postfix_open(previous_kind, current)
        {
            return false;
        }

        if is_unary_plus_or_minus(previous_kind, self.previous_tokens[previous_index]) {
            return false;
        }

        if (is_contextual_spacing(previous_kind) && self.is_compact(previous_index))
            || (is_contextual_spacing(current)
                && !is_unary_plus_or_minus(current, self.previous_tokens[current_index])
                && self.is_compact(current_index))
        {
            return false;
        }

        self.pending_horizontal_space || needs_space(previous_kind, current)
    }

    fn is_compact(&self, index: usize) -> bool {
        !matches!(
            index.checked_sub(1).and_then(|before| self.lexemes.get(before)),
            Some(lexeme) if lexeme.kind() == LexemeKind::Trivia(TriviaKind::Whitespace)
        ) && !matches!(
            self.lexemes.get(index + 1),
            Some(lexeme) if lexeme.kind() == LexemeKind::Trivia(TriviaKind::Whitespace)
        )
    }
}

#[derive(Clone, Copy)]
enum Content {
    Token { index: usize, kind: TokenKind },
    Comment,
}

fn previous_tokens(lexemes: &[Lexeme]) -> Vec<Option<TokenKind>> {
    let mut previous = None;
    lexemes
        .iter()
        .map(|lexeme| {
            let result = previous;
            if let LexemeKind::Token(kind) = lexeme.kind() {
                previous = Some(kind);
            }
            result
        })
        .collect()
}

fn suppresses_space_before(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::StringText
            | TokenKind::InterpolationStart
            | TokenKind::InterpolationEnd
            | TokenKind::StringEnd
            | TokenKind::Symbol(
                Symbol::RightParen
                    | Symbol::RightBracket
                    | Symbol::RightBrace
                    | Symbol::Comma
                    | Symbol::Colon
                    | Symbol::Semicolon
                    | Symbol::Dot
                    | Symbol::QuestionDot
                    | Symbol::Question
                    | Symbol::BangBang
                    | Symbol::ColonColon
            )
    )
}

fn suppresses_space_after(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::StringStart
            | TokenKind::StringText
            | TokenKind::InterpolationStart
            | TokenKind::InterpolationEnd
            | TokenKind::Symbol(
                Symbol::LeftParen
                    | Symbol::LeftBracket
                    | Symbol::LeftBrace
                    | Symbol::At
                    | Symbol::Dot
                    | Symbol::QuestionDot
                    | Symbol::ColonColon
                    | Symbol::Bang
                    | Symbol::Ampersand
            )
    )
}

fn is_string_join(previous: TokenKind, current: TokenKind) -> bool {
    matches!(
        (previous, current),
        (
            TokenKind::StringStart | TokenKind::StringText | TokenKind::InterpolationEnd,
            TokenKind::StringText | TokenKind::InterpolationStart | TokenKind::StringEnd
        ) | (TokenKind::InterpolationStart, TokenKind::InterpolationEnd)
    )
}

fn is_postfix_open(previous: TokenKind, current: TokenKind) -> bool {
    matches!(
        current,
        TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket)
    ) && can_end_expression(previous)
}

fn needs_space(previous: TokenKind, current: TokenKind) -> bool {
    if matches!(
        previous,
        TokenKind::Symbol(Symbol::Comma | Symbol::Colon | Symbol::Semicolon)
    ) {
        return true;
    }
    if is_operator(previous) || is_operator(current) {
        return true;
    }
    is_word_like(previous) && is_word_like(current)
}

fn is_word_like(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::IntegerLiteral(_)
            | TokenKind::FloatLiteral(_)
            | TokenKind::CharLiteral
            | TokenKind::StringStart
            | TokenKind::StringEnd
            | TokenKind::Keyword(_)
            | TokenKind::ReservedWord(_)
    )
}

fn is_operator(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Symbol(
            Symbol::QuestionColon
                | Symbol::Arrow
                | Symbol::Star
                | Symbol::Slash
                | Symbol::Percent
                | Symbol::Plus
                | Symbol::Minus
                | Symbol::DotDot
                | Symbol::DotDotLess
                | Symbol::Less
                | Symbol::Greater
                | Symbol::LessEqual
                | Symbol::GreaterEqual
                | Symbol::EqualEqual
                | Symbol::BangEqual
                | Symbol::AndAnd
                | Symbol::OrOr
                | Symbol::PlusEqual
                | Symbol::MinusEqual
                | Symbol::StarEqual
                | Symbol::SlashEqual
                | Symbol::PercentEqual
                | Symbol::Equal
        ) | TokenKind::Keyword(Keyword::In | Keyword::Is | Keyword::As)
    )
}

fn is_contextual_spacing(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Symbol(Symbol::Less | Symbol::Greater | Symbol::Plus | Symbol::Minus)
    )
}

fn is_unary_plus_or_minus(kind: TokenKind, previous: Option<TokenKind>) -> bool {
    matches!(kind, TokenKind::Symbol(Symbol::Plus | Symbol::Minus))
        && previous.is_none_or(|kind| !can_end_expression(kind))
}

fn can_end_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::IntegerLiteral(_)
            | TokenKind::FloatLiteral(_)
            | TokenKind::CharLiteral
            | TokenKind::StringEnd
            | TokenKind::Keyword(
                Keyword::False | Keyword::Null | Keyword::Super | Keyword::This | Keyword::True
            )
            | TokenKind::Symbol(
                Symbol::RightParen
                    | Symbol::RightBracket
                    | Symbol::RightBrace
                    | Symbol::Question
                    | Symbol::BangBang
            )
    )
}

fn closing_symbol(kind: TokenKind) -> Option<Symbol> {
    let TokenKind::Symbol(
        symbol @ (Symbol::RightParen | Symbol::RightBracket | Symbol::RightBrace),
    ) = kind
    else {
        return None;
    };
    Some(symbol)
}

fn is_opening(symbol: Symbol) -> bool {
    matches!(
        symbol,
        Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace
    )
}

fn is_closing(symbol: Symbol) -> bool {
    matches!(
        symbol,
        Symbol::RightParen | Symbol::RightBracket | Symbol::RightBrace
    )
}

fn delimiters_match(opening: Symbol, closing: Symbol) -> bool {
    matches!(
        (opening, closing),
        (Symbol::LeftParen, Symbol::RightParen)
            | (Symbol::LeftBracket, Symbol::RightBracket)
            | (Symbol::LeftBrace, Symbol::RightBrace)
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        diagnostic::Diagnostic,
        source::{SourceId, SourceMap},
    };

    use super::{FormattingError, format_source};

    fn formatted(text: &str) -> String {
        let mut sources = SourceMap::new();
        let source_id = sources.add_source("test.ko", text).expect("unique source");
        format_source(&sources, source_id).expect("valid source")
    }

    fn diagnostic_codes(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect()
    }

    #[test]
    fn formats_spacing_indentation_and_contextual_adjacency() {
        let source = concat!(
            "package  sample . core\n",
            "fun  <T> id(own item:T):T {\n",
            "return  item\n",
            "}\n",
            "fun signs():Int {\n",
            "val x=-1\n",
            "return x+ 2\n",
            "}\n",
        );
        assert_eq!(
            formatted(source),
            concat!(
                "package sample.core\n",
                "fun < T > id(own item: T): T {\n",
                "    return item\n",
                "}\n",
                "fun signs(): Int {\n",
                "    val x = -1\n",
                "    return x + 2\n",
                "}\n",
            )
        );
        assert_eq!(
            formatted("val x:List<Int>  =listOf(1)"),
            "val x: List<Int > = listOf(1)"
        );
    }

    #[test]
    fn preserves_comments_newlines_strings_and_is_idempotent() {
        let source = concat!(
            "fun f():Unit {\r\n",
            "\tval text=\"a  ${1+2}  b\"/* exact\ncomment */\r\n",
            "\t// line  text\r\n",
            "}\r\n",
        );
        let first = formatted(source);
        assert_eq!(
            first,
            concat!(
                "fun f(): Unit {\r\n",
                "    val text = \"a  ${1+2}  b\" /* exact\ncomment */\r\n",
                "    // line  text\r\n",
                "}\r\n",
            )
        );
        assert_eq!(formatted(&first), first);
    }

    #[test]
    fn preserves_explicit_instance_receiver_mode_tokens() {
        let source = "class C{public override own fun consume():Unit{}}";
        let expected = "class C{public override own fun consume(): Unit{}}";
        let first = formatted(source);
        assert_eq!(first, expected);
        assert_eq!(formatted(&first), first);
    }

    #[test]
    fn rejects_user_diagnostics_and_foreign_source_identity() {
        let mut invalid_sources = SourceMap::new();
        let invalid = invalid_sources
            .add_source("invalid.ko", "val value = $")
            .expect("unique source");
        let error = format_source(&invalid_sources, invalid).expect_err("invalid source");
        assert!(matches!(
            error,
            FormattingError::Diagnostics(ref diagnostics)
                if diagnostic_codes(diagnostics) == ["L0001"]
                    && diagnostics[0].primary_span().start() == 12
        ));

        let mut parser_sources = SourceMap::new();
        let parser_error = parser_sources
            .add_source("parser-error.ko", "val item =")
            .expect("unique source");
        let error = format_source(&parser_sources, parser_error).expect_err("invalid syntax");
        assert!(
            matches!(
                &error,
                FormattingError::Diagnostics(diagnostics)
                    if diagnostic_codes(diagnostics) == ["L0009"]
                        && diagnostics[0].primary_span().start() == 10
                        && diagnostics[0].primary_span().is_empty()
            ),
            "{error:?}"
        );

        let mut first = SourceMap::new();
        let foreign: SourceId = first.add_source("first.ko", "").expect("unique source");
        let second = SourceMap::new();
        assert!(matches!(
            format_source(&second, foreign),
            Err(FormattingError::Source(_))
        ));
    }

    #[test]
    fn removes_horizontal_space_from_empty_and_blank_lines() {
        assert_eq!(formatted(" \t\n\t\r\n"), "\n\r\n");
    }

    #[test]
    fn deeply_nested_delimiters_use_iterative_formatting_state() {
        let depth = 128;
        let source = format!(
            "fun f(): Unit {{\nval item = {}1{}\n}}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let result = formatted(&source);
        assert!(result.contains("\n    val item = "));
        assert_eq!(formatted(&result), result);
    }
}
