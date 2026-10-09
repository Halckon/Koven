use std::{collections::BTreeMap, error::Error, fmt};

use crate::{
    diagnostic::{Diagnostic, DiagnosticCodeError, DiagnosticError, Severity},
    name_resolution::{ExternalSymbolId, NameResolution, NameResolutionError, Namespace},
    source::Span,
};

use super::{
    CompilationUnitIndex, CompilationUnitInputError, DeclarationId, PackageId, SourceUnitId,
    UnitDiagnosticOrderError, UnitSymbolId,
};

/// compilation-unit 名称解析入口的内部或输入契约错误。
#[derive(Debug)]
pub enum CompilationUnitNameError {
    /// 输入无法重新建立与 index 相同的规范 compilation unit。
    Input(CompilationUnitInputError),
    /// 调用方传入的 index 与 source inputs 不属于同一分析输入。
    MismatchedIndex,
    /// 单文件名称解析发生内部失败。
    Name(NameResolutionError),
    /// unit 诊断无法建立稳定全序。
    DiagnosticOrder(UnitDiagnosticOrderError),
    /// 生产诊断码目录无效。
    DiagnosticCode(DiagnosticCodeError),
    /// unit 诊断无法由受检模型构造。
    Diagnostic(DiagnosticError),
}
impl fmt::Display for CompilationUnitNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => write!(formatter, "invalid compilation-unit input: {error}"),
            Self::MismatchedIndex => formatter
                .write_str("compilation-unit index does not belong to the supplied source inputs"),
            Self::Name(error) => write!(formatter, "name resolution failed: {error}"),
            Self::DiagnosticOrder(error) => {
                write!(formatter, "unit diagnostic ordering failed: {error}")
            }
            Self::DiagnosticCode(error) => write!(formatter, "invalid diagnostic catalog: {error}"),
            Self::Diagnostic(error) => write!(formatter, "invalid unit diagnostic: {error}"),
        }
    }
}
impl Error for CompilationUnitNameError {}
impl From<CompilationUnitInputError> for CompilationUnitNameError {
    fn from(error: CompilationUnitInputError) -> Self {
        Self::Input(error)
    }
}
impl From<NameResolutionError> for CompilationUnitNameError {
    fn from(error: NameResolutionError) -> Self {
        Self::Name(error)
    }
}
impl From<UnitDiagnosticOrderError> for CompilationUnitNameError {
    fn from(error: UnitDiagnosticOrderError) -> Self {
        Self::DiagnosticOrder(error)
    }
}
impl From<DiagnosticCodeError> for CompilationUnitNameError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}
impl From<DiagnosticError> for CompilationUnitNameError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}

/// unit reference 的目标；package/source 声明不得伪装成 external symbol。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitReferenceTarget {
    /// 限定路径中的 package segment。
    Package(PackageId),
    /// 可跨文件引用的唯一顶层声明。
    Declaration(DeclarationId),
    /// 同一 package 内的有序函数 overload set。
    OverloadSet(Vec<DeclarationId>),
    /// 文件局部 symbol。
    Symbol(UnitSymbolId),
    /// 文件局部有序 symbol 集合。
    Symbols(Vec<UnitSymbolId>),
    /// compiler-bound 外部 symbol。
    External(ExternalSymbolId),
    /// compiler-bound 外部函数 overload set。
    ExternalOverloadSet(Vec<ExternalSymbolId>),
    /// 当前 unit 中未解析。
    Unresolved,
    /// 同一顺序作用域稍后出现的 local。
    LaterLocal(Span),
}

/// import、限定路径或普通 Identifier 的 unit-global reference fact。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNameReference {
    source_unit: SourceUnitId,
    span: Span,
    namespace: Option<Namespace>,
    target: UnitReferenceTarget,
}
impl UnitNameReference {
    pub(super) fn new(
        source_unit: SourceUnitId,
        span: Span,
        namespace: Option<Namespace>,
        target: UnitReferenceTarget,
    ) -> Self {
        Self {
            source_unit,
            span,
            namespace,
            target,
        }
    }
    /// 返回引用所属 source unit。
    #[must_use]
    pub const fn source_unit(&self) -> SourceUnitId {
        self.source_unit
    }
    /// 返回真实 Identifier Span。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// 返回查询命名空间；package segment 为 `None`。
    #[must_use]
    pub const fn namespace(&self) -> Option<Namespace> {
        self.namespace
    }
    /// 返回 unit-global 目标。
    #[must_use]
    pub const fn target(&self) -> &UnitReferenceTarget {
        &self.target
    }
}

/// 一个 source unit 的文件局部名称产物。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnitNames {
    source_unit: SourceUnitId,
    resolution: NameResolution,
}
impl SourceUnitNames {
    pub(super) const fn new(source_unit: SourceUnitId, resolution: NameResolution) -> Self {
        Self {
            source_unit,
            resolution,
        }
    }
    /// 返回 source-unit identity。
    #[must_use]
    pub const fn source_unit(&self) -> SourceUnitId {
        self.source_unit
    }
    /// 返回保留旧 `ReferenceTarget` 语义的文件局部产物。
    #[must_use]
    pub const fn resolution(&self) -> &NameResolution {
        &self.resolution
    }
}

/// SPEC-0025 的 recovery 名称产物。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitNames {
    index: CompilationUnitIndex,
    source_units: Vec<SourceUnitNames>,
    declaration_symbols: BTreeMap<DeclarationId, UnitSymbolId>,
    references: Vec<UnitNameReference>,
    value_lookup_hints: Vec<UnitNameReference>,
    diagnostics: Vec<Diagnostic>,
}
impl CompilationUnitNames {
    pub(super) fn new(
        index: CompilationUnitIndex,
        source_units: Vec<SourceUnitNames>,
        declaration_symbols: BTreeMap<DeclarationId, UnitSymbolId>,
        references: Vec<UnitNameReference>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            index,
            source_units,
            declaration_symbols,
            references,
            value_lookup_hints: Vec::new(),
            diagnostics,
        }
    }
    pub(super) fn with_value_lookup_hints(mut self, hints: Vec<UnitNameReference>) -> Self {
        self.value_lookup_hints = hints;
        self
    }
    pub(crate) fn value_lookup_hints(&self) -> &[UnitNameReference] {
        &self.value_lookup_hints
    }

    /// 返回共同拥有的 package/source/declaration index。
    #[must_use]
    pub const fn index(&self) -> &CompilationUnitIndex {
        &self.index
    }
    /// 返回 SourceUnitId 顺序的文件局部产物。
    #[must_use]
    pub fn source_units(&self) -> &[SourceUnitNames] {
        &self.source_units
    }
    /// 返回规范 declaration identity 到真实文件局部 symbol identity 的完整映射。
    #[must_use]
    pub const fn declaration_symbols(&self) -> &BTreeMap<DeclarationId, UnitSymbolId> {
        &self.declaration_symbols
    }
    /// 查询一个顶层声明对应的真实文件局部 symbol identity。
    #[must_use]
    pub fn declaration_symbol(&self, declaration: DeclarationId) -> Option<UnitSymbolId> {
        self.declaration_symbols.get(&declaration).copied()
    }
    /// 返回稳定 source/Span 顺序的 unit reference facts。
    #[must_use]
    pub fn references(&self) -> &[UnitNameReference] {
        &self.references
    }
    /// 返回聚合后的稳定 unit 诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// 只在全部 parser/index/name/import 诊断无 error 时发布 validated view。
    pub fn validate(self) -> Result<ValidatedCompilationUnitNames, Box<Self>> {
        if self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            Err(Box::new(self))
        } else {
            Ok(ValidatedCompilationUnitNames(self))
        }
    }
}

/// 不可伪造的无错误 compilation-unit 名称产物。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCompilationUnitNames(CompilationUnitNames);
impl ValidatedCompilationUnitNames {
    /// 返回只读 recovery product。
    #[must_use]
    pub const fn names(&self) -> &CompilationUnitNames {
        &self.0
    }
    /// 解包为 recovery product。
    #[must_use]
    pub fn into_names(self) -> CompilationUnitNames {
        self.0
    }
}
