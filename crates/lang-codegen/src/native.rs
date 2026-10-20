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
    name_resolution::{DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{
        ConstEnabledOwnedUnit, ConstOwnedCompilationUnitView, OwnedCompilationUnitView,
        OwnedCompilationUnitViewError, ValidatedCompilationUnitOwnership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ConstEnabledTypedUnit, IntrinsicTypeConstructor,
        ParameterMode, TypeEnvironment, UnitCallableTarget, UnitTypeKind,
        ValidatedCompilationUnitTypes,
    },
};

use crate::{
    llvm,
    llvm::entry::NativeEntryPlan,
    ssa::{LoweringError, LoweringErrorKind, unit_lower::lower_owned_unit_with_entry},
};

static NEXT_UNIT_OBJECT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// 已由 project entry resolver 选择的 compilation-unit process entry。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeUnitEntry {
    /// `() -> Unit` entry。
    NoArguments(DeclarationId),
    /// shared Borrow `(Array<String>) -> Unit` entry。
    BorrowedArguments(DeclarationId),
}

impl NativeUnitEntry {
    const fn declaration(self) -> DeclarationId {
        match self {
            Self::NoArguments(declaration) | Self::BorrowedArguments(declaration) => declaration,
        }
    }
}

impl From<DeclarationId> for NativeUnitEntry {
    fn from(declaration: DeclarationId) -> Self {
        Self::NoArguments(declaration)
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
    /// entry 不属于当前输入的唯一受支持非泛型顶层 process callable。
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

/// 把 validated compilation-unit analysis chain 和已解析的显式 process entry 原子写为 object。
///
/// 所有 frontend、SSA、target layout 与 LLVM 验证均在 sibling temporary 上完成；只有完整 object
/// 生成成功后才替换 `output`。失败时既有目标保持不变，临时文件由本函数清理。
///
/// 常量专用 typed capability 不能绕过独立 unit constant ownership/native 交接。
/// ```compile_fail,E0308
/// use std::path::Path;
/// use lang_codegen::{emit_native_unit_object, NativeUnitEntry};
/// use lang_frontend::{source::SourceMap, name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
///     type_checking::{ConstEnabledTypedUnit, TypeEnvironment}, ownership_checking::ValidatedCompilationUnitOwnership};
/// fn reject(sources: &SourceMap, inputs: &[SourceUnitInput<'_>], names: &ValidatedCompilationUnitNames,
///     environment: &TypeEnvironment, typed: &ConstEnabledTypedUnit, owned: &ValidatedCompilationUnitOwnership,
///     entry: NativeUnitEntry, output: &Path) {
///     emit_native_unit_object(sources, inputs, names, environment, typed, owned, entry, output);
/// }
/// ```
/// 常量专用 owned capability 也不能进入普通 unit native 入口。
/// ```compile_fail,E0308
/// use std::path::Path;
/// use lang_codegen::{emit_native_unit_object, NativeUnitEntry};
/// use lang_frontend::{source::SourceMap, name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
///     type_checking::{ValidatedCompilationUnitTypes, TypeEnvironment}, ownership_checking::ConstEnabledOwnedUnit};
/// fn reject(sources: &SourceMap, inputs: &[SourceUnitInput<'_>], names: &ValidatedCompilationUnitNames,
///     environment: &TypeEnvironment, typed: &ValidatedCompilationUnitTypes, owned: &ConstEnabledOwnedUnit,
///     entry: NativeUnitEntry, output: &Path) {
///     emit_native_unit_object(sources, inputs, names, environment, typed, owned, entry, output);
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub fn emit_native_unit_object(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: impl Into<NativeUnitEntry>,
    output: &Path,
) -> Result<(), NativeObjectError> {
    let entry = entry.into();
    let unit = owned_compilation_unit_view(sources, inputs, names, environment, typed, owned)?;
    emit_native_owned_unit_object(&unit, entry, output)
}

/// 从已封闭的普通 owned-unit 借用生成原子发布的 native object。
///
/// 本入口不重建来源 index；entry shape、verified SSA 和 native entry plan 在 reserve 前检查。
/// LLVM/layout/object emission 仍在 sibling temporary 上执行，成功后才原子提交。
pub fn emit_native_owned_unit_object(
    unit: &OwnedCompilationUnitView<'_, '_>,
    entry: impl Into<NativeUnitEntry>,
    output: &Path,
) -> Result<(), NativeObjectError> {
    let entry = entry.into();
    let sources = unit.sources();
    validate_unit_entry(unit.names(), unit.types(), entry)?;
    let (program, function) =
        lower_owned_unit_with_entry(unit, entry.declaration()).map_err(map_lowering_error)?;
    let plan = native_unit_entry_plan(&program, entry, function)?;
    let temporary = SiblingObject::reserve(output)?;
    llvm::emit_verified_object(&program, sources, plan, temporary.path())
        .map_err(|error| map_backend_error(sources, &program, error))?;
    temporary.commit(output)
}

/// 从同一次常量 typed/owned capability 生成原子发布的 compilation-unit object。
/// 身份校验及 SSA lowering 在创建临时输出前完成；入口与发布流程复用基础 native 契约。
///
/// 基础 typed 或 owned 不能替代常量专用 capability。
/// ```compile_fail,E0308
/// use std::path::Path;
/// use lang_codegen::{emit_native_constant_unit_object, NativeUnitEntry};
/// use lang_frontend::{source::SourceMap, name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
///     type_checking::{ValidatedCompilationUnitTypes, TypeEnvironment}, ownership_checking::ConstEnabledOwnedUnit};
/// fn reject(sources: &SourceMap, inputs: &[SourceUnitInput<'_>], names: &ValidatedCompilationUnitNames,
///     environment: &TypeEnvironment, typed: &ValidatedCompilationUnitTypes, owned: &ConstEnabledOwnedUnit,
///     entry: NativeUnitEntry, output: &Path) {
///     emit_native_constant_unit_object(sources, inputs, names, environment, typed, owned, entry, output);
/// }
/// ```
/// ```compile_fail,E0308
/// use std::path::Path;
/// use lang_codegen::{emit_native_constant_unit_object, NativeUnitEntry};
/// use lang_frontend::{source::SourceMap, name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
///     type_checking::{ConstEnabledTypedUnit, TypeEnvironment}, ownership_checking::ValidatedCompilationUnitOwnership};
/// fn reject(sources: &SourceMap, inputs: &[SourceUnitInput<'_>], names: &ValidatedCompilationUnitNames,
///     environment: &TypeEnvironment, typed: &ConstEnabledTypedUnit, owned: &ValidatedCompilationUnitOwnership,
///     entry: NativeUnitEntry, output: &Path) {
///     emit_native_constant_unit_object(sources, inputs, names, environment, typed, owned, entry, output);
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub fn emit_native_constant_unit_object(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ConstEnabledTypedUnit,
    owned: &ConstEnabledOwnedUnit,
    entry: impl Into<NativeUnitEntry>,
    output: &Path,
) -> Result<(), NativeObjectError> {
    let entry = entry.into();
    let unit =
        const_owned_compilation_unit_view(sources, inputs, names, environment, typed, owned)?;
    emit_native_const_owned_unit_object(&unit, entry, output)
}

/// 从已封闭的常量专用 owned-unit 借用生成原子发布的 native object。
///
/// 本入口保留完整物化与短路能力，不重建来源 index；entry/SSA 检查先于 reserve，
/// LLVM/layout/object emission 仍在 sibling temporary 上完成，成功后才原子提交。
pub fn emit_native_const_owned_unit_object(
    unit: &ConstOwnedCompilationUnitView<'_, '_>,
    entry: impl Into<NativeUnitEntry>,
    output: &Path,
) -> Result<(), NativeObjectError> {
    let entry = entry.into();
    let sources = unit.sources();
    validate_unit_entry(unit.names(), unit.types(), entry)?;
    let (program, function) = crate::ssa::unit_lower::constant::lower_const_owned_unit_with_entry(
        unit,
        entry.declaration(),
    )
    .map_err(map_lowering_error)?;
    let plan = native_unit_entry_plan(&program, entry, function)?;
    let temporary = SiblingObject::reserve(output)?;
    llvm::emit_verified_object(&program, sources, plan, temporary.path())
        .map_err(|error| map_backend_error(sources, &program, error))?;
    temporary.commit(output)
}

fn validate_unit_entry(
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    entry: NativeUnitEntry,
) -> Result<(), NativeObjectError> {
    let declaration_id = entry.declaration();
    let declaration = names
        .names()
        .index()
        .declarations()
        .get(declaration_id.index())
        .ok_or_else(|| invalid_entry(None))?;
    let callable = typed
        .signatures()
        .declaration(declaration_id)
        .and_then(|signature| signature.callable())
        .ok_or_else(|| invalid_entry(Some(declaration.name_span())))?;
    if callable.target() != UnitCallableTarget::Declaration(declaration_id)
        || !callable.type_parameters().is_empty()
        || typed.types().get(callable.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
    {
        return Err(invalid_entry(Some(declaration.name_span())));
    }
    let valid_parameters = match entry {
        NativeUnitEntry::NoArguments(_) => callable.parameters().is_empty(),
        NativeUnitEntry::BorrowedArguments(_) => match callable.parameters() {
            [parameter] if parameter.mode() == ParameterMode::Borrow => matches!(
                typed.types().get(parameter.ty()),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Array,
                    arguments,
                }) if matches!(arguments.as_slice(), [string]
                    if typed.types().get(*string)
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
            ),
            _ => false,
        },
    };
    valid_parameters
        .then_some(())
        .ok_or_else(|| invalid_entry(Some(declaration.name_span())))
}

fn native_unit_entry_plan(
    program: &crate::ssa::model::Program,
    entry: NativeUnitEntry,
    function: crate::ssa::model::FunctionId,
) -> Result<NativeEntryPlan, NativeObjectError> {
    match entry {
        NativeUnitEntry::NoArguments(_) => Ok(NativeEntryPlan::NoArguments { function }),
        NativeUnitEntry::BorrowedArguments(_) => borrowed_arguments_entry_plan(program, function),
    }
}

fn borrowed_arguments_entry_plan(
    program: &crate::ssa::model::Program,
    function: crate::ssa::model::FunctionId,
) -> Result<NativeEntryPlan, NativeObjectError> {
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

fn invalid_entry(span: Option<Span>) -> NativeObjectError {
    NativeObjectError {
        kind: NativeObjectErrorKind::InvalidEntry,
        span,
        detail: "entry must match its declared native process shape and return Unit".to_owned(),
        diagnostic: None,
    }
}

impl From<OwnedCompilationUnitViewError> for NativeObjectError {
    fn from(error: OwnedCompilationUnitViewError) -> Self {
        map_lowering_error(error.into())
    }
}

fn map_lowering_error(error: LoweringError) -> NativeObjectError {
    let kind = match error.kind {
        LoweringErrorKind::MismatchedSource | LoweringErrorKind::MismatchedAnalysis => {
            NativeObjectErrorKind::MismatchedAnalysis
        }
        LoweringErrorKind::FrontendDiagnostics => NativeObjectErrorKind::FrontendDiagnostics,
        LoweringErrorKind::BlockingDeferred => NativeObjectErrorKind::BlockingDeferred,
        LoweringErrorKind::UnsupportedNode
        | LoweringErrorKind::UnsupportedCompanionCall
        | LoweringErrorKind::InstanceLimitExceeded => NativeObjectErrorKind::UnsupportedSource,
        LoweringErrorKind::MissingFact
        | LoweringErrorKind::InvalidLiteral
        | LoweringErrorKind::InvalidModel
        | LoweringErrorKind::InvalidSsa => NativeObjectErrorKind::InvalidModel,
    };
    NativeObjectError {
        kind,
        span: error.span,
        detail: if error.kind == LoweringErrorKind::UnsupportedCompanionCall {
            "companion function calls are not supported by native code generation".to_owned()
        } else {
            format!("frontend lowering failed with {:?}", error.kind)
        },
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
