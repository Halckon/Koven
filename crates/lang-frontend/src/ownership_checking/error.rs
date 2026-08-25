use std::{error::Error, fmt};

use crate::{
    ast::AstError,
    diagnostic::{DiagnosticCodeError, DiagnosticError},
    source::SourceError,
};

/// 所有权阶段的编译器内部错误；用户程序错误进入结构化诊断产物。
#[derive(Debug)]
pub enum OwnershipCheckingError {
    /// ParsedFile 与 NameResolution 不属于同一源码。
    MismatchedNameSource,
    /// ParsedFile 与 TypedFile 不属于同一源码。
    MismatchedTypedSource,
    /// NameResolution 与 TypedFile 不属于同一分析身份链。
    MismatchedAnalysisIdentity,
    /// TypedFile 中的 construction descriptor 违反 Phase 2 产物不变量。
    InvalidConstructionDescriptor {
        /// 无效 descriptor 的 expression arena 下标。
        expression: usize,
    },
    /// AST typed ID 不满足 Parser 前置不变量。
    Ast(AstError),
    /// 输入源码或 Span 不满足前置不变量。
    Source(SourceError),
    /// 生产诊断码目录无效。
    DiagnosticCode(DiagnosticCodeError),
    /// 无法构造合法结构化诊断。
    Diagnostic(DiagnosticError),
}

impl fmt::Display for OwnershipCheckingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MismatchedNameSource => {
                formatter.write_str("name resolution belongs to a different source")
            }
            Self::MismatchedTypedSource => {
                formatter.write_str("typed file belongs to a different source")
            }
            Self::MismatchedAnalysisIdentity => {
                formatter.write_str("name and typed files belong to different analyses")
            }
            Self::InvalidConstructionDescriptor { expression } => {
                write!(
                    formatter,
                    "invalid construction descriptor for expression {expression}"
                )
            }
            Self::Ast(error) => write!(formatter, "ownership AST error: {error}"),
            Self::Source(error) => write!(formatter, "ownership source error: {error}"),
            Self::DiagnosticCode(error) => {
                write!(formatter, "ownership diagnostic code error: {error}")
            }
            Self::Diagnostic(error) => write!(formatter, "ownership diagnostic error: {error}"),
        }
    }
}

impl Error for OwnershipCheckingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ast(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::DiagnosticCode(error) => Some(error),
            Self::Diagnostic(error) => Some(error),
            Self::MismatchedNameSource
            | Self::MismatchedTypedSource
            | Self::MismatchedAnalysisIdentity
            | Self::InvalidConstructionDescriptor { .. } => None,
        }
    }
}

impl From<AstError> for OwnershipCheckingError {
    fn from(error: AstError) -> Self {
        Self::Ast(error)
    }
}

impl From<SourceError> for OwnershipCheckingError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<DiagnosticCodeError> for OwnershipCheckingError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}

impl From<DiagnosticError> for OwnershipCheckingError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}
