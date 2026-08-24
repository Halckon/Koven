//! 已完成 frontend analysis 到首个本机 object 的 workspace 公共边界。

use std::{fmt, path::Path};

use lang_frontend::{
    name_resolution::{NameResolution, ScopeKind, SymbolId, SymbolKind},
    ownership_checking::OwnershipCheckedFile,
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{BuiltinType, TypeKind, TypedFile},
};

use crate::{
    llvm,
    ssa::{LoweringError, LoweringErrorKind, lower_scalar_file_with_entry},
};

/// 源码 analysis 到 object 失败的稳定 workspace 分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeObjectErrorKind {
    /// frontend 产物不属于同一 source/analysis chain。
    MismatchedAnalysis,
    /// 任一 frontend 阶段已经发布用户诊断。
    FrontendDiagnostics,
    /// 所有权阶段仍含阻止 codegen 的 deferred 节点。
    BlockingDeferred,
    /// 当前封闭的 frontend→SSA 子集不支持该源码。
    UnsupportedSource,
    /// entry 不属于本文件的唯一非泛型顶层 `() -> Unit` callable。
    InvalidEntry,
    /// frontend facts 或 SSA 内部不变量损坏。
    InvalidModel,
    /// LLVM target、验证或 object emission 失败。
    Backend,
}

/// 一次本机 object emission 的结构化错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeObjectError {
    kind: NativeObjectErrorKind,
    span: Option<Span>,
    detail: String,
}

impl NativeObjectError {
    /// 返回不依赖本地化文本的错误分类。
    #[must_use]
    pub const fn kind(&self) -> NativeObjectErrorKind {
        self.kind
    }

    /// 返回可用的源位置；纯 backend 失败可能没有位置。
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
}

impl fmt::Display for NativeObjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native object {:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for NativeObjectError {}

/// 把一条匹配的 frontend analysis chain 和显式 resolved entry 生成首个 target object。
///
/// 调用方继续拥有全部输入、输出路径和链接步骤；本函数不读取源码文件、不创建临时目录，也
/// 不按名称选择 entry。
pub fn emit_native_object(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    entry: SymbolId,
    output: &Path,
) -> Result<(), NativeObjectError> {
    validate_entry(names, typed, entry)?;
    let (program, entry) =
        lower_scalar_file_with_entry(sources, parsed, names, typed, owned, entry)
            .map_err(map_lowering_error)?;
    llvm::emit_verified_object(&program, sources, entry, output).map_err(|error| {
        NativeObjectError {
            kind: NativeObjectErrorKind::Backend,
            span: None,
            detail: format!("{error:?}"),
        }
    })
}

fn validate_entry(
    names: &NameResolution,
    typed: &TypedFile,
    entry: SymbolId,
) -> Result<(), NativeObjectError> {
    let symbol = names
        .symbols()
        .get(entry.index())
        .ok_or_else(|| invalid_entry(None))?;
    let scope = names
        .scopes()
        .get(symbol.scope().index())
        .ok_or_else(|| invalid_entry(Some(symbol.span())))?;
    let callable = typed
        .callables()
        .iter()
        .find(|callable| callable.symbol() == entry && callable.owner().is_none())
        .ok_or_else(|| invalid_entry(Some(symbol.span())))?;
    let is_unit = matches!(
        typed.types().get(callable.return_type()),
        Some(TypeKind::Builtin(BuiltinType::Unit))
    );
    if symbol.kind() != SymbolKind::Function
        || scope.kind() != ScopeKind::File
        || scope.parent().is_some()
        || !callable.type_parameters().is_empty()
        || !callable.parameters().is_empty()
        || !is_unit
    {
        return Err(invalid_entry(Some(symbol.span())));
    }
    Ok(())
}

fn invalid_entry(span: Option<Span>) -> NativeObjectError {
    NativeObjectError {
        kind: NativeObjectErrorKind::InvalidEntry,
        span,
        detail: "entry must identify one top-level non-generic () -> Unit function".to_owned(),
    }
}

fn map_lowering_error(error: LoweringError) -> NativeObjectError {
    let kind = match error.kind {
        LoweringErrorKind::MismatchedSource | LoweringErrorKind::MismatchedAnalysis => {
            NativeObjectErrorKind::MismatchedAnalysis
        }
        LoweringErrorKind::FrontendDiagnostics => NativeObjectErrorKind::FrontendDiagnostics,
        LoweringErrorKind::BlockingDeferred => NativeObjectErrorKind::BlockingDeferred,
        LoweringErrorKind::UnsupportedNode | LoweringErrorKind::InstanceLimitExceeded => {
            NativeObjectErrorKind::UnsupportedSource
        }
        LoweringErrorKind::MissingFact
        | LoweringErrorKind::InvalidLiteral
        | LoweringErrorKind::InvalidModel
        | LoweringErrorKind::InvalidSsa => NativeObjectErrorKind::InvalidModel,
    };
    NativeObjectError {
        kind,
        span: error.span,
        detail: format!("frontend lowering failed with {:?}", error.kind),
    }
}
