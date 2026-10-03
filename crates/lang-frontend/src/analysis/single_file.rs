use std::{error::Error, fmt};

use crate::{
    diagnostic::Diagnostic,
    lexer::{LexerInternalError, lex},
    name_resolution::{NameEnvironment, NameResolution, NameResolutionError, resolve_names},
    ownership_checking::{OwnershipCheckedFile, OwnershipCheckingError, check_ownership},
    parser::{ParsedFile, ParserInternalError, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{TypeCheckingError, TypeEnvironment, TypedFile, check_types},
};

/// 完成后可由宿主检查原始诊断的单文件阶段；标签本身不授予语义能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SingleFileStage {
    /// 词法阶段。
    Lexer,
    /// 文件语法阶段（诊断已包含 Lexer 诊断）。
    Parser,
    /// 名称解析阶段。
    NameResolution,
    /// 类型检查阶段。
    TypeChecking,
    /// 所有权检查阶段。
    OwnershipChecking,
}

/// typed gate 成功后、ownership 开始前的同轮只读投影；不代表无诊断或已验证能力。
pub struct SingleFileTypedView<'a> {
    parsed: &'a ParsedFile,
    names: &'a NameResolution,
    typed: &'a TypedFile,
}

impl<'a> SingleFileTypedView<'a> {
    /// 返回本轮的原语法产物。
    #[must_use]
    pub const fn parsed(&self) -> &'a ParsedFile {
        self.parsed
    }

    /// 返回本轮的原名称 recovery facts。
    #[must_use]
    pub const fn names(&self) -> &'a NameResolution {
        self.names
    }

    /// 返回本轮的原类型 recovery facts。
    #[must_use]
    pub const fn typed(&self) -> &'a TypedFile {
        self.typed
    }
}

/// 按值拥有同轮原始产物与宿主观察结果；不提供 basic/const/backend validated 能力。
#[derive(Debug)]
pub struct SingleFileAnalysis<T> {
    parsed: ParsedFile,
    names: NameResolution,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
    observed: T,
}

impl<T> SingleFileAnalysis<T> {
    /// 返回原语法产物（包含词法诊断）。
    #[must_use]
    pub const fn parsed(&self) -> &ParsedFile {
        &self.parsed
    }

    /// 返回原名称 recovery facts。
    #[must_use]
    pub const fn names(&self) -> &NameResolution {
        &self.names
    }

    /// 返回原类型 recovery facts。
    #[must_use]
    pub const fn typed(&self) -> &TypedFile {
        &self.typed
    }

    /// 返回原所有权 facts，包括诊断及 deferred。
    #[must_use]
    pub const fn owned(&self) -> &OwnershipCheckedFile {
        &self.owned
    }

    /// 返回 typed 阶段接缝生成的宿主观察结果。
    #[must_use]
    pub const fn observed(&self) -> &T {
        &self.observed
    }

    /// 消费式取回原产物；不克隆、不重建身份，也不验证或升级能力。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        ParsedFile,
        NameResolution,
        TypedFile,
        OwnershipCheckedFile,
        T,
    ) {
        (
            self.parsed,
            self.names,
            self.typed,
            self.owned,
            self.observed,
        )
    }
}

/// 固定链的原始内部失败，或阶段 gate / typed observer 原样返回的宿主失败。
#[derive(Debug)]
pub enum SingleFileAnalysisError<E> {
    /// 词法内部失败或输入 SourceId 不属于 SourceMap。
    Lexer(LexerInternalError),
    /// 语法内部失败。
    Parser(ParserInternalError),
    /// 名称内部失败。
    Name(NameResolutionError),
    /// 类型内部失败，包括原环境身份不匹配。
    Type(TypeCheckingError),
    /// 所有权内部失败。
    Ownership(OwnershipCheckingError),
    /// 宿主 gate 或 typed observer 请求停止。
    Host(E),
}

impl<E: fmt::Display> fmt::Display for SingleFileAnalysisError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lexer(error) => error.fmt(formatter),
            Self::Parser(error) => error.fmt(formatter),
            Self::Name(error) => error.fmt(formatter),
            Self::Type(error) => error.fmt(formatter),
            Self::Ownership(error) => error.fmt(formatter),
            Self::Host(error) => error.fmt(formatter),
        }
    }
}

impl<E: Error + 'static> Error for SingleFileAnalysisError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Lexer(error) => error,
            Self::Parser(error) => error,
            Self::Name(error) => error,
            Self::Type(error) => error,
            Self::Ownership(error) => error,
            Self::Host(error) => error,
        })
    }
}

/// 在原 SourceMap 和显式环境上推进单文件五阶段，不执行 IO 或选择宿主恢复策略。
///
/// 每阶段后立即调用 gate，失败不运行下一阶段。typed gate 成功后恰一次调用 observer，
/// 成功才进入 ownership。observer 的临时只读 view 不能作为 T/E 逃逸；产物按值移动。
/// 不提前检查环境、不聚合诊断、不 validate facts；原 checker 决定身份错误与恢复行为。
pub fn analyze_single_file<T, E>(
    sources: &SourceMap,
    source: SourceId,
    name_environment: &NameEnvironment,
    type_environment: &TypeEnvironment,
    mut gate: impl FnMut(SingleFileStage, &[Diagnostic]) -> Result<(), E>,
    typed_observer: impl for<'a> FnOnce(SingleFileTypedView<'a>) -> Result<T, E>,
) -> Result<SingleFileAnalysis<T>, SingleFileAnalysisError<E>> {
    let lexed = lex(sources, source).map_err(SingleFileAnalysisError::Lexer)?;
    gate(SingleFileStage::Lexer, lexed.diagnostics()).map_err(SingleFileAnalysisError::Host)?;
    let parsed = parse_file(sources, &lexed).map_err(SingleFileAnalysisError::Parser)?;
    gate(SingleFileStage::Parser, parsed.diagnostics()).map_err(SingleFileAnalysisError::Host)?;
    let names =
        resolve_names(sources, &parsed, name_environment).map_err(SingleFileAnalysisError::Name)?;
    gate(SingleFileStage::NameResolution, names.diagnostics())
        .map_err(SingleFileAnalysisError::Host)?;
    let typed = check_types(sources, &parsed, &names, type_environment)
        .map_err(SingleFileAnalysisError::Type)?;
    gate(SingleFileStage::TypeChecking, typed.diagnostics())
        .map_err(SingleFileAnalysisError::Host)?;
    let observed = typed_observer(SingleFileTypedView {
        parsed: &parsed,
        names: &names,
        typed: &typed,
    })
    .map_err(SingleFileAnalysisError::Host)?;
    let owned = check_ownership(sources, &parsed, &names, &typed)
        .map_err(SingleFileAnalysisError::Ownership)?;
    gate(SingleFileStage::OwnershipChecking, owned.diagnostics())
        .map_err(SingleFileAnalysisError::Host)?;
    Ok(SingleFileAnalysis {
        parsed,
        names,
        typed,
        owned,
        observed,
    })
}
