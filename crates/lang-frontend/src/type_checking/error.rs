use std::{error::Error, fmt, io};

use crate::{
    ast::AstError,
    diagnostic::{DiagnosticCodeError, DiagnosticError},
    source::SourceError,
};

/// 基础类型检查阶段的内部失败。
#[derive(Debug)]
pub enum TypeCheckingError {
    /// AST typed ID 或节点关系无效。
    Ast(AstError),
    /// SourceMap 不拥有输入 SourceId 或 Span。
    Source(SourceError),
    /// ParsedFile 与 NameResolution 不属于同一 source。
    MismatchedNameSource,
    /// TypeEnvironment 与 NameResolution 使用了不同的 NameEnvironment 身份。
    MismatchedNameEnvironment,
    /// 外部类型绑定引用了不存在或类别不相容的 symbol。
    InvalidExternalBinding,
    /// 生产诊断码目录无效。
    DiagnosticCode(DiagnosticCodeError),
    /// 类型诊断无法由受检模型构造。
    Diagnostic(DiagnosticError),
    /// 无法创建固定工作栈线程。
    CheckerThread(io::ErrorKind),
    /// 类型检查 worker 意外 panic。
    CheckerThreadPanicked,
}

impl fmt::Display for TypeCheckingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ast(error) => write!(f, "invalid parser AST: {error}"),
            Self::Source(error) => write!(f, "invalid source identity or span: {error}"),
            Self::MismatchedNameSource => {
                f.write_str("parsed file and name resolution use different sources")
            }
            Self::MismatchedNameEnvironment => {
                f.write_str("type and name environments do not share an identity")
            }
            Self::InvalidExternalBinding => {
                f.write_str("external type binding does not match its name symbol")
            }
            Self::DiagnosticCode(error) => write!(f, "invalid diagnostic catalog: {error}"),
            Self::Diagnostic(error) => write!(f, "invalid type diagnostic: {error}"),
            Self::CheckerThread(kind) => {
                write!(f, "could not create type checker worker: {kind:?}")
            }
            Self::CheckerThreadPanicked => f.write_str("type checker worker panicked"),
        }
    }
}

impl Error for TypeCheckingError {}

impl From<AstError> for TypeCheckingError {
    fn from(value: AstError) -> Self {
        Self::Ast(value)
    }
}

impl From<SourceError> for TypeCheckingError {
    fn from(value: SourceError) -> Self {
        Self::Source(value)
    }
}

impl From<DiagnosticCodeError> for TypeCheckingError {
    fn from(value: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(value)
    }
}

impl From<DiagnosticError> for TypeCheckingError {
    fn from(value: DiagnosticError) -> Self {
        Self::Diagnostic(value)
    }
}
