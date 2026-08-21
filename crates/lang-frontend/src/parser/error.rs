use std::{error::Error, fmt};

use crate::{
    ast::AstError,
    diagnostic::{DiagnosticCodeError, DiagnosticError},
    source::SourceError,
};

/// Parser 内部边界失败；不表示 Koven 用户语法错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParserInternalError {
    /// Source identity 或范围不变量失败。
    Source(SourceError),
    /// 索引式 AST 不变量失败。
    Ast(AstError),
    /// 生产错误码目录不变量失败。
    DiagnosticCode(DiagnosticCodeError),
    /// 结构化诊断构造或排序不变量失败。
    Diagnostic(DiagnosticError),
    /// 词法产物不满足公开 Lexer 不变量。
    InvalidLexemeStream,
    /// 无法创建隔离 Parser 递归栈的工作线程。
    ParserThread(std::io::ErrorKind),
    /// Parser 工作线程因编译器缺陷而 panic。
    ParserThreadPanicked,
    /// 输入超过当前实现可安全处理的内部递归预算。
    NestingLimitExceeded {
        /// 当前实现允许的内部递归预算单位。
        limit: usize,
    },
}

impl fmt::Display for ParserInternalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "parser source error: {error}"),
            Self::Ast(error) => write!(formatter, "parser AST error: {error}"),
            Self::DiagnosticCode(error) => {
                write!(formatter, "parser diagnostic code error: {error}")
            }
            Self::Diagnostic(error) => write!(formatter, "parser diagnostic error: {error}"),
            Self::InvalidLexemeStream => {
                formatter.write_str("parser received an invalid lexeme stream")
            }
            Self::ParserThread(kind) => {
                write!(formatter, "failed to create parser worker thread: {kind:?}")
            }
            Self::ParserThreadPanicked => formatter.write_str("parser worker thread panicked"),
            Self::NestingLimitExceeded { limit } => {
                write!(
                    formatter,
                    "parser exceeds the implementation recursion budget of {limit} units"
                )
            }
        }
    }
}

impl Error for ParserInternalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Ast(error) => Some(error),
            Self::DiagnosticCode(error) => Some(error),
            Self::Diagnostic(error) => Some(error),
            Self::InvalidLexemeStream
            | Self::ParserThread(_)
            | Self::ParserThreadPanicked
            | Self::NestingLimitExceeded { .. } => None,
        }
    }
}

impl From<SourceError> for ParserInternalError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<AstError> for ParserInternalError {
    fn from(error: AstError) -> Self {
        Self::Ast(error)
    }
}

impl From<DiagnosticCodeError> for ParserInternalError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}

impl From<DiagnosticError> for ParserInternalError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}
