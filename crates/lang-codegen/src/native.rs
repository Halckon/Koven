//! 已完成 frontend analysis 到首个本机 object 的 workspace 公共边界。

#[cfg(test)]
mod unit_tests;

use std::{
    fmt, fs,
    fs::OpenOptions,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::{
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        DeclarationId, NameResolution, ScopeKind, SourceUnitInput, SymbolId, SymbolKind,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{OwnershipCheckedFile, ValidatedCompilationUnitOwnership},
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, FunctionParameterType, IntrinsicTypeConstructor, ParameterMode,
        TypeEnvironment, TypeKind, TypedFile, UnitCallableTarget, UnitTypeKind,
        ValidatedCompilationUnitTypes,
    },
};

use crate::{
    llvm,
    llvm::entry::NativeEntryPlan,
    ssa::{
        LoweringError, LoweringErrorKind, lower_scalar_file_with_entry,
        unit_lower::lower_scalar_unit_with_entry, unit_plan::validate_unit_inputs,
    },
};

static NEXT_UNIT_OBJECT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// 已由调用方完成名称选择的封闭 native process entry shape。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEntry {
    /// 显式或 conventional `() -> Unit` entry。
    NoArguments(SymbolId),
    /// Conventional shared Borrow `Array<String> -> Unit` entry。
    BorrowedArguments(SymbolId),
}

impl NativeEntry {
    const fn symbol(self) -> SymbolId {
        match self {
            Self::NoArguments(symbol) | Self::BorrowedArguments(symbol) => symbol,
        }
    }
}

impl From<SymbolId> for NativeEntry {
    fn from(symbol: SymbolId) -> Self {
        Self::NoArguments(symbol)
    }
}

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
    /// 当前 target 无法表示来源类型的存储布局。
    TargetLayout,
}

/// 一次本机 object emission 的结构化错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeObjectError {
    kind: NativeObjectErrorKind,
    span: Option<Span>,
    detail: String,
    diagnostic: Option<Box<Diagnostic>>,
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

    /// 返回可直接交给现有 renderer 的用户诊断。
    #[must_use]
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        self.diagnostic.as_deref()
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
    entry: impl Into<NativeEntry>,
    output: &Path,
) -> Result<(), NativeObjectError> {
    let entry = entry.into();
    validate_entry(names, typed, entry)?;
    let (program, function) =
        lower_scalar_file_with_entry(sources, parsed, names, typed, owned, entry.symbol())
            .map_err(map_lowering_error)?;
    let plan = native_entry_plan(&program, entry, function)?;
    llvm::emit_verified_object(&program, sources, plan, output)
        .map_err(|error| map_backend_error(sources, &program, error))
}

/// 把 validated compilation-unit analysis chain 和显式顶层 `() -> Unit` entry 原子写为 object。
///
/// 所有 frontend、SSA、target layout 与 LLVM 验证均在 sibling temporary 上完成；只有完整 object
/// 生成成功后才替换 `output`。失败时既有目标保持不变，临时文件由本函数清理。
#[allow(clippy::too_many_arguments)]
pub fn emit_native_unit_object(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
    output: &Path,
) -> Result<(), NativeObjectError> {
    validate_unit_inputs(sources, inputs, names, environment, typed, owned)
        .map_err(map_lowering_error)?;
    validate_unit_entry(names, typed, entry)?;
    let (program, function) =
        lower_scalar_unit_with_entry(sources, inputs, names, environment, typed, owned, entry)
            .map_err(map_lowering_error)?;
    let plan = NativeEntryPlan::NoArguments { function };
    let temporary = SiblingObject::reserve(output)?;
    llvm::emit_verified_object(&program, sources, plan, temporary.path())
        .map_err(|error| map_backend_error(sources, &program, error))?;
    temporary.commit(output)
}

fn validate_entry(
    names: &NameResolution,
    typed: &TypedFile,
    entry: NativeEntry,
) -> Result<(), NativeObjectError> {
    let entry_symbol = entry.symbol();
    let symbol = names
        .symbols()
        .get(entry_symbol.index())
        .ok_or_else(|| invalid_entry(None))?;
    let scope = names
        .scopes()
        .get(symbol.scope().index())
        .ok_or_else(|| invalid_entry(Some(symbol.span())))?;
    let callable = typed
        .callables()
        .iter()
        .find(|callable| callable.symbol() == entry_symbol && callable.owner().is_none())
        .ok_or_else(|| invalid_entry(Some(symbol.span())))?;
    let is_unit = matches!(
        typed.types().get(callable.return_type()),
        Some(TypeKind::Builtin(BuiltinType::Unit))
    );
    if symbol.kind() != SymbolKind::Function
        || scope.kind() != ScopeKind::File
        || scope.parent().is_some()
        || !callable.type_parameters().is_empty()
        || !is_unit
    {
        return Err(invalid_entry(Some(symbol.span())));
    }
    let valid_parameters = match entry {
        NativeEntry::NoArguments(_) => callable.parameters().is_empty(),
        NativeEntry::BorrowedArguments(_) => match callable.parameters() {
            [
                FunctionParameterType {
                    mode: ParameterMode::Borrow,
                    ty,
                },
            ] => matches!(
                typed.types().get(*ty),
                Some(TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Array,
                    arguments,
                }) if matches!(arguments.as_slice(), [string]
                    if typed.types().get(*string) == Some(&TypeKind::Builtin(BuiltinType::String)))
            ),
            _ => false,
        },
    };
    if !valid_parameters {
        return Err(invalid_entry(Some(symbol.span())));
    }
    Ok(())
}

fn validate_unit_entry(
    names: &ValidatedCompilationUnitNames,
    typed: &ValidatedCompilationUnitTypes,
    entry: DeclarationId,
) -> Result<(), NativeObjectError> {
    let declaration = names
        .names()
        .index()
        .declarations()
        .get(entry.index())
        .ok_or_else(|| invalid_entry(None))?;
    let callable = typed
        .types()
        .signatures()
        .declaration(entry)
        .and_then(|signature| signature.callable())
        .ok_or_else(|| invalid_entry(Some(declaration.name_span())))?;
    if callable.target() != UnitCallableTarget::Declaration(entry)
        || !callable.type_parameters().is_empty()
        || !callable.parameters().is_empty()
        || typed.types().types().get(callable.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
    {
        return Err(invalid_entry(Some(declaration.name_span())));
    }
    Ok(())
}

fn native_entry_plan(
    program: &crate::ssa::model::Program,
    entry: NativeEntry,
    function: crate::ssa::model::FunctionId,
) -> Result<NativeEntryPlan, NativeObjectError> {
    match entry {
        NativeEntry::NoArguments(_) => Ok(NativeEntryPlan::NoArguments { function }),
        NativeEntry::BorrowedArguments(_) => {
            let module = program
                .module(function.module())
                .ok_or_else(|| invalid_entry(None))?;
            let function_data = module
                .function(function)
                .ok_or_else(|| invalid_entry(None))?;
            let parameter = function_data
                .blocks
                .first()
                .and_then(|block| block.parameters.first())
                .copied()
                .ok_or_else(|| invalid_entry(None))?;
            let crate::ssa::model::EntityType::Loan {
                kind: crate::ssa::model::LoanKind::Shared,
                target: arguments,
            } = function_data
                .entity(parameter)
                .map(|entity| entity.ty)
                .ok_or_else(|| invalid_entry(None))?
            else {
                return Err(invalid_entry(None));
            };
            let (crate::ssa::model::SequentialContainerKind::Array, string) = module
                .sequential_container(arguments)
                .ok_or_else(|| invalid_entry(None))?
            else {
                return Err(invalid_entry(None));
            };
            Ok(NativeEntryPlan::BorrowedArguments {
                function,
                arguments,
                string,
            })
        }
    }
}

fn invalid_entry(span: Option<Span>) -> NativeObjectError {
    NativeObjectError {
        kind: NativeObjectErrorKind::InvalidEntry,
        span,
        detail: "entry must match its declared native process shape and return Unit".to_owned(),
        diagnostic: None,
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
        diagnostic: None,
    }
}

pub(crate) fn map_backend_error(
    sources: &SourceMap,
    program: &crate::ssa::model::Program,
    error: llvm::LlvmAdapterError,
) -> NativeObjectError {
    let llvm::LlvmAdapterError::InvalidLayout(layout) = error else {
        return NativeObjectError {
            kind: NativeObjectErrorKind::Backend,
            span: None,
            detail: format!("{error:?}"),
            diagnostic: None,
        };
    };
    let Some(origin) = program.type_origin(layout.ty) else {
        return NativeObjectError {
            kind: NativeObjectErrorKind::Backend,
            span: None,
            detail: format!("source origin missing for {layout:?}"),
            diagnostic: None,
        };
    };
    let diagnostic = (|| {
        let catalog = codes::catalog().ok()?;
        let code = catalog.resolve(codes::TARGET_LAYOUT).ok()?;
        let mut diagnostic = Diagnostic::new(
            sources,
            Severity::Error,
            code,
            "类型布局超出当前编译目标的表示能力",
            origin.primary,
        )
        .ok()?;
        if origin.declaration != origin.primary {
            diagnostic
                .add_label(sources, origin.declaration, "该类型在此声明")
                .ok()?;
        }
        diagnostic
            .add_note(format!(
                "目标布局检查失败：{:?} {:?}",
                layout.quantity, layout.failure
            ))
            .ok()?;
        Some(diagnostic)
    })();
    NativeObjectError {
        kind: NativeObjectErrorKind::TargetLayout,
        span: Some(origin.primary),
        detail: format!("{layout:?}"),
        diagnostic: diagnostic.map(Box::new),
    }
}

struct SiblingObject {
    path: Option<PathBuf>,
}

impl SiblingObject {
    fn reserve(output: &Path) -> Result<Self, NativeObjectError> {
        let parent = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        loop {
            let sequence = NEXT_UNIT_OBJECT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!(
                ".koven-unit-object-{}-{sequence}.tmp",
                std::process::id()
            ));
            if candidate == output {
                continue;
            }
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(_) => {
                    return Ok(Self {
                        path: Some(candidate),
                    });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(backend_io_error("reserve sibling object", error));
                }
            }
        }
    }

    fn path(&self) -> &Path {
        self.path.as_deref().expect("live sibling object")
    }

    fn commit(mut self, output: &Path) -> Result<(), NativeObjectError> {
        fs::rename(self.path(), output)
            .map_err(|error| backend_io_error("commit sibling object", error))?;
        self.path = None;
        Ok(())
    }
}

impl Drop for SiblingObject {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn backend_io_error(operation: &str, error: std::io::Error) -> NativeObjectError {
    NativeObjectError {
        kind: NativeObjectErrorKind::Backend,
        span: None,
        detail: format!("{operation}: {error}"),
        diagnostic: None,
    }
}
