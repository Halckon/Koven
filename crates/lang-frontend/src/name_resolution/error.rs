use std::{error::Error, fmt, io};

use crate::{
    ast::AstError,
    diagnostic::{DiagnosticCodeError, DiagnosticError},
    source::SourceError,
};

/// 名称解析阶段的内部失败。
#[derive(Debug)]
pub enum NameResolutionError {
    /// AST typed ID 或节点关系无效。
    Ast(AstError),
    /// SourceMap 不拥有输入 SourceId 或 Span。
    Source(SourceError),
    /// 生产诊断码目录无效。
    DiagnosticCode(DiagnosticCodeError),
    /// 名称诊断无法由受检模型构造。
    Diagnostic(DiagnosticError),
    /// 无法创建固定工作栈线程。
    ResolverThread(io::ErrorKind),
    /// 名称解析 worker 意外 panic。
    ResolverThreadPanicked,
}

impl fmt::Display for NameResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ast(e) => write!(f, "invalid parser AST: {e}"),
            Self::Source(e) => write!(f, "invalid source identity or span: {e}"),
            Self::DiagnosticCode(e) => write!(f, "invalid diagnostic catalog: {e}"),
            Self::Diagnostic(e) => write!(f, "invalid name diagnostic: {e}"),
            Self::ResolverThread(kind) => {
                write!(f, "could not create name resolver worker: {kind:?}")
            }
            Self::ResolverThreadPanicked => f.write_str("name resolver worker panicked"),
        }
    }
}
impl Error for NameResolutionError {}
impl From<AstError> for NameResolutionError {
    fn from(v: AstError) -> Self {
        Self::Ast(v)
    }
}
impl From<SourceError> for NameResolutionError {
    fn from(v: SourceError) -> Self {
        Self::Source(v)
    }
}
impl From<DiagnosticCodeError> for NameResolutionError {
    fn from(v: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(v)
    }
}
impl From<DiagnosticError> for NameResolutionError {
    fn from(v: DiagnosticError) -> Self {
        Self::Diagnostic(v)
    }
}
