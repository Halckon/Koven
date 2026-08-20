//! Koven v1 的确定性词法模型与入口。

mod scanner;

use std::{error::Error, fmt};

use crate::{
    diagnostic::{Diagnostic, DiagnosticCodeError, DiagnosticError},
    source::{SourceError, SourceId, SourceMap, Span},
};

/// 对一份已加载源码执行完整词法分析。
///
/// 用户源码错误进入返回产物的诊断序列；只有 source identity 或内部诊断构造不变量失败才
/// 返回 [`LexerInternalError`]。
pub fn lex(sources: &SourceMap, source_id: SourceId) -> Result<LexedFile, LexerInternalError> {
    scanner::scan(sources, source_id)
}

/// 一份源码的完整、按源码顺序排列的词法产物。
#[derive(Debug)]
pub struct LexedFile {
    source_id: SourceId,
    lexemes: Vec<Lexeme>,
    diagnostics: Vec<Diagnostic>,
}

impl LexedFile {
    /// 返回产物所属的 source identity。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// 返回包含 trivia、非法区域和唯一 EOF 的完整 lexeme 流。
    #[must_use]
    pub fn lexemes(&self) -> &[Lexeme] {
        &self.lexemes
    }

    /// 返回既定确定性全序下的用户诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// 一个词法分类及其原始 UTF-8 字节范围。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lexeme {
    kind: LexemeKind,
    span: Span,
}

impl Lexeme {
    /// 返回词法分类。
    #[must_use]
    pub const fn kind(self) -> LexemeKind {
        self.kind
    }

    /// 返回原始源码范围。
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }
}

/// 完整 lexeme 流中的顶层分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexemeKind {
    /// 普通语言 token。
    Token(TokenKind),
    /// parser 可跳过但 formatter 可保留的 trivia。
    Trivia(TriviaKind),
    /// 已诊断并消费的非法源码区域。
    Invalid(InvalidKind),
    /// 输入末尾的唯一空范围标记。
    Eof,
}

/// Koven v1 的普通 token 分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// 普通标识符；软关键字也使用此分类。
    Identifier,
    /// 十进制整数字面量。
    IntegerLiteral,
    /// 十进制小数字面量。
    FloatLiteral,
    /// 合法的单 scalar 字符字面量。
    CharLiteral,
    /// 字符串开始引号。
    StringStart,
    /// 字符串内最大非空文本段。
    StringText,
    /// `${` 插值开始符。
    InterpolationStart,
    /// 与插值开始符匹配的 `}`。
    InterpolationEnd,
    /// 字符串结束引号。
    StringEnd,
    /// 当前版本硬关键字。
    Keyword(Keyword),
    /// 当前版本禁止使用的未来保留字。
    ReservedWord(ReservedWord),
    /// 固定运算符或标点。
    Symbol(Symbol),
}

/// 42 个硬关键字。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    /// `class`。
    Class,
    /// `companion`。
    Companion,
    /// `const`。
    Const,
    /// `enum`。
    Enum,
    /// `extern`。
    Extern,
    /// `fun`。
    Fun,
    /// `import`。
    Import,
    /// `interface`。
    Interface,
    /// `object`。
    Object,
    /// `package`。
    Package,
    /// `typealias`。
    Typealias,
    /// `val`。
    Val,
    /// `value`。
    Value,
    /// `var`。
    Var,
    /// `vararg`。
    Vararg,
    /// `break`。
    Break,
    /// `continue`。
    Continue,
    /// `else`。
    Else,
    /// `for`。
    For,
    /// `if`。
    If,
    /// `in`。
    In,
    /// `is`。
    Is,
    /// `loop`。
    Loop,
    /// `return`。
    Return,
    /// `when`。
    When,
    /// `while`。
    While,
    /// `borrow`。
    Borrow,
    /// `inout`。
    Inout,
    /// `move`。
    Move,
    /// `own`。
    Own,
    /// `unsafe`。
    Unsafe,
    /// `internal`。
    Internal,
    /// `private`。
    Private,
    /// `public`。
    Public,
    /// `as`。
    As,
    /// `false`。
    False,
    /// `null`。
    Null,
    /// `operator`。
    Operator,
    /// `override`。
    Override,
    /// `super`。
    Super,
    /// `this`。
    This,
    /// `true`。
    True,
}

/// 11 个未来保留字。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservedWord {
    /// `async`。
    Async,
    /// `await`。
    Await,
    /// `suspend`。
    Suspend,
    /// `actor`。
    Actor,
    /// `spawn`。
    Spawn,
    /// `sealed`。
    Sealed,
    /// `dyn`。
    Dyn,
    /// `where`。
    Where,
    /// `yield`。
    Yield,
    /// `macro`。
    Macro,
    /// `reify`。
    Reify,
}

/// 固定运算符和标点；名称描述拼写而不提前表达 parser 语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    /// `(`。
    LeftParen,
    /// `)`。
    RightParen,
    /// `[`。
    LeftBracket,
    /// `]`。
    RightBracket,
    /// `{`。
    LeftBrace,
    /// `}`。
    RightBrace,
    /// `,`。
    Comma,
    /// `:`。
    Colon,
    /// `;`。
    Semicolon,
    /// `@`。
    At,
    /// `.`。
    Dot,
    /// `?.`。
    QuestionDot,
    /// `?`。
    Question,
    /// `?:`。
    QuestionColon,
    /// `!!`。
    BangBang,
    /// `!`。
    Bang,
    /// `::`。
    ColonColon,
    /// `->`。
    Arrow,
    /// `*`。
    Star,
    /// `/`。
    Slash,
    /// `%`。
    Percent,
    /// `+`。
    Plus,
    /// `-`。
    Minus,
    /// `..`。
    DotDot,
    /// `..<`。
    DotDotLess,
    /// `<`。
    Less,
    /// `>`。
    Greater,
    /// `<=`。
    LessEqual,
    /// `>=`。
    GreaterEqual,
    /// `==`。
    EqualEqual,
    /// `!=`。
    BangEqual,
    /// `&`。
    Ampersand,
    /// `&&`。
    AndAnd,
    /// `||`。
    OrOr,
    /// `+=`。
    PlusEqual,
    /// `-=`。
    MinusEqual,
    /// `*=`。
    StarEqual,
    /// `/=`。
    SlashEqual,
    /// `%=`。
    PercentEqual,
    /// `=`。
    Equal,
    /// `as?`。
    AsQuestion,
    /// `!in`。
    BangIn,
    /// `!is`。
    BangIs,
}

/// 可跳过但仍保留原始范围的 trivia 分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    /// 最大连续 space / tab 区域。
    Whitespace,
    /// 一个 LF 或 CRLF 换行序列。
    Newline,
    /// 不包含终止换行的 `//` 注释。
    LineComment,
    /// 已终止的 `/* ... */` 注释。
    BlockComment,
}

/// 必须占据自身字节区域的非法 lexeme 分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidKind {
    /// 一个不属于任何合法 token 的 Unicode scalar。
    UnexpectedCharacter,
    /// 从 `/*` 延伸到 EOF 的未终止注释。
    UnterminatedBlockComment,
    /// 字符串中的非法转义区域。
    InvalidStringEscape,
    /// 一个整体非法的字符字面量区域。
    InvalidCharLiteral,
    /// 一个整体非法的数字区域。
    InvalidNumericLiteral,
}

/// Lexer 内部边界失败；不表示 Koven 用户源码错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LexerInternalError {
    /// Source identity 或范围不变量失败。
    Source(SourceError),
    /// 生产错误码目录不变量失败。
    DiagnosticCode(DiagnosticCodeError),
    /// 结构化诊断构造或排序不变量失败。
    Diagnostic(DiagnosticError),
}

impl fmt::Display for LexerInternalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "lexer source error: {error}"),
            Self::DiagnosticCode(error) => {
                write!(formatter, "lexer diagnostic code error: {error}")
            }
            Self::Diagnostic(error) => write!(formatter, "lexer diagnostic error: {error}"),
        }
    }
}

impl Error for LexerInternalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::DiagnosticCode(error) => Some(error),
            Self::Diagnostic(error) => Some(error),
        }
    }
}

impl From<SourceError> for LexerInternalError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<DiagnosticCodeError> for LexerInternalError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}

impl From<DiagnosticError> for LexerInternalError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}
