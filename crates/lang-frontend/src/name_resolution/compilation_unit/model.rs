use crate::{
    ast::ItemId,
    diagnostic::{Diagnostic, Severity},
    name_resolution::{Namespace, SymbolId, SymbolKind},
    parser::ParsedFile,
    source::{SourceId, Span},
};

macro_rules! define_unit_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub(super) usize);
        impl $name {
            /// 返回本次 compilation-unit 索引内的规范下标。
            #[must_use]
            pub const fn index(self) -> usize {
                self.0
            }
        }
    };
}

define_unit_id!(PackageId, "package 在单次 compilation-unit 索引中的身份。");
define_unit_id!(SourceUnitId, "源码单元在规范排序后的身份。");
define_unit_id!(DeclarationId, "可作为跨文件引用目标的顶层声明身份。");

/// compilation unit 中一个文件局部 symbol 的稳定身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitSymbolId {
    source_unit: SourceUnitId,
    symbol: SymbolId,
}
impl UnitSymbolId {
    pub(super) const fn new(source_unit: SourceUnitId, symbol: SymbolId) -> Self {
        Self {
            source_unit,
            symbol,
        }
    }
    /// 返回 symbol 所属 source unit。
    #[must_use]
    pub const fn source_unit(self) -> SourceUnitId {
        self.source_unit
    }
    /// 返回所属文件内的 symbol identity。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.symbol
    }
}

/// 调用方提供的稳定、不透明 source-root 身份。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRootIdentity(String);
impl SourceRootIdentity {
    /// 保存调用方提供的不透明稳定 identity。
    #[must_use]
    pub fn new(identity: impl Into<String>) -> Self {
        Self(identity.into())
    }
    /// 返回未经展示路径改写的 identity。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 已验证的 root-relative UTF-8 逻辑路径。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogicalSourcePath(String);
impl LogicalSourcePath {
    pub(super) fn from_validated(path: String) -> Self {
        Self(path)
    }
    /// 返回使用 `/` 分隔的规范相对路径。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub(super) fn package_segments(&self) -> impl Iterator<Item = &str> {
        self.0
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .split('/')
            .filter(|segment| !segment.is_empty())
    }
}

/// compilation-unit 输入中的稳定源码键。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceUnitKey {
    root: SourceRootIdentity,
    logical_path: LogicalSourcePath,
}
impl SourceUnitKey {
    pub(super) const fn new(root: SourceRootIdentity, logical_path: LogicalSourcePath) -> Self {
        Self { root, logical_path }
    }
    /// 返回 source root identity。
    #[must_use]
    pub const fn root(&self) -> &SourceRootIdentity {
        &self.root
    }
    /// 返回 root 内逻辑路径。
    #[must_use]
    pub const fn logical_path(&self) -> &LogicalSourcePath {
        &self.logical_path
    }
}

/// 一份尚未规范排序的源码单元输入。
#[derive(Clone, Copy, Debug)]
pub struct SourceUnitInput<'parsed> {
    root_identity: &'parsed str,
    logical_path: &'parsed str,
    source_id: SourceId,
    parsed: &'parsed ParsedFile,
}
impl<'parsed> SourceUnitInput<'parsed> {
    /// 构造一项显式 source-unit 输入；全部不变量由索引入口统一校验。
    #[must_use]
    pub const fn new(
        root_identity: &'parsed str,
        logical_path: &'parsed str,
        source_id: SourceId,
        parsed: &'parsed ParsedFile,
    ) -> Self {
        Self {
            root_identity,
            logical_path,
            source_id,
            parsed,
        }
    }
    /// 返回调用方提供的 root identity。
    #[must_use]
    pub const fn root_identity(self) -> &'parsed str {
        self.root_identity
    }
    /// 返回调用方提供的 root 内逻辑路径。
    #[must_use]
    pub const fn logical_path(self) -> &'parsed str {
        self.logical_path
    }
    /// 返回输入源码在共同 SourceMap 中的身份。
    #[must_use]
    pub const fn source_id(self) -> SourceId {
        self.source_id
    }
    /// 返回该源码的解析产物。
    #[must_use]
    pub const fn parsed(self) -> &'parsed ParsedFile {
        self.parsed
    }
}

/// 结构化 package identity；空 segment 序列表示默认 package。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(Vec<String>);
impl PackageName {
    pub(super) fn new(segments: Vec<String>) -> Self {
        Self(segments)
    }
    /// 返回源码顺序的 package segment。
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.0
    }
    /// 返回该名称是否表示默认 package。
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.0.is_empty()
    }
}

/// 一个按 package name 规范排序的 package 记录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    id: PackageId,
    name: PackageName,
}
impl Package {
    pub(super) const fn new(id: PackageId, name: PackageName) -> Self {
        Self { id, name }
    }
    /// 返回本次索引内的 package identity。
    #[must_use]
    pub const fn id(&self) -> PackageId {
        self.id
    }
    /// 返回结构化 package name。
    #[must_use]
    pub const fn name(&self) -> &PackageName {
        &self.name
    }
}

/// 一个规范排序并已关联 package 的源码单元。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnit {
    id: SourceUnitId,
    key: SourceUnitKey,
    source_id: SourceId,
    package: PackageId,
}
impl SourceUnit {
    pub(super) const fn new(
        id: SourceUnitId,
        key: SourceUnitKey,
        source_id: SourceId,
        package: PackageId,
    ) -> Self {
        Self {
            id,
            key,
            source_id,
            package,
        }
    }
    /// 返回本次索引内的 source-unit identity。
    #[must_use]
    pub const fn id(&self) -> SourceUnitId {
        self.id
    }
    /// 返回稳定 source key。
    #[must_use]
    pub const fn key(&self) -> &SourceUnitKey {
        &self.key
    }
    /// 返回共同 SourceMap 中的 source identity。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }
    /// 返回所属 package identity。
    #[must_use]
    pub const fn package(&self) -> PackageId {
        self.package
    }
}

/// 顶层声明的有效可见性；省略修饰符时为 `Public`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclarationVisibility {
    /// 其他 package 可见。
    Public,
    /// 当前 compilation unit 内可见。
    Internal,
    /// 仅声明 source unit 内可见。
    Private,
}

/// Stage 1 顶层声明；Stage 2 才将它映射到真实文件局部 `SymbolId`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDeclaration {
    id: DeclarationId,
    source_unit: SourceUnitId,
    root: ItemId,
    package: PackageId,
    name: String,
    name_span: Span,
    namespace: Namespace,
    kind: SymbolKind,
    visibility: DeclarationVisibility,
}
impl UnitDeclaration {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        id: DeclarationId,
        source_unit: SourceUnitId,
        root: ItemId,
        package: PackageId,
        name: String,
        name_span: Span,
        namespace: Namespace,
        kind: SymbolKind,
        visibility: DeclarationVisibility,
    ) -> Self {
        Self {
            id,
            source_unit,
            root,
            package,
            name,
            name_span,
            namespace,
            kind,
            visibility,
        }
    }
    /// 返回规范 declaration identity。
    #[must_use]
    pub const fn id(&self) -> DeclarationId {
        self.id
    }
    /// 返回声明所属 source unit。
    #[must_use]
    pub const fn source_unit(&self) -> SourceUnitId {
        self.source_unit
    }
    /// 返回声明在所属 ParsedFile 中的 root ItemId。
    #[must_use]
    pub const fn root(&self) -> ItemId {
        self.root
    }
    /// 返回声明所属 package。
    #[must_use]
    pub const fn package(&self) -> PackageId {
        self.package
    }
    /// 返回声明名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// 返回声明名称的精确源码范围。
    #[must_use]
    pub const fn name_span(&self) -> Span {
        self.name_span
    }
    /// 返回声明所在的独立命名空间。
    #[must_use]
    pub const fn namespace(&self) -> Namespace {
        self.namespace
    }
    /// 返回与单文件 resolver 一致的声明类别。
    #[must_use]
    pub const fn kind(&self) -> SymbolKind {
        self.kind
    }
    /// 返回声明的有效顶层可见性。
    #[must_use]
    pub const fn visibility(&self) -> DeclarationVisibility {
        self.visibility
    }
}

/// L0146 构造所需的 package/path 不一致事实。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackagePathMismatch {
    source_unit: SourceUnitId,
    expected: PackageName,
    declared: Option<PackageName>,
    primary_span: Span,
}
impl PackagePathMismatch {
    pub(super) const fn new(
        source_unit: SourceUnitId,
        expected: PackageName,
        declared: Option<PackageName>,
        primary_span: Span,
    ) -> Self {
        Self {
            source_unit,
            expected,
            declared,
            primary_span,
        }
    }
    /// 返回发生不匹配的 source unit。
    #[must_use]
    pub const fn source_unit(&self) -> SourceUnitId {
        self.source_unit
    }
    /// 返回由逻辑父目录确定的 package。
    #[must_use]
    pub const fn expected(&self) -> &PackageName {
        &self.expected
    }
    /// 返回源码声明的 package；省略 directive 时为 None。
    #[must_use]
    pub const fn declared(&self) -> Option<&PackageName> {
        self.declared.as_ref()
    }
    /// 返回 L0146 的主范围。
    #[must_use]
    pub const fn primary_span(&self) -> Span {
        self.primary_span
    }
}

/// Stage 1 recovery product：稳定 unit/package/declaration index 与 L0146/L0147 诊断。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitIndex {
    packages: Vec<Package>,
    source_units: Vec<SourceUnit>,
    declarations: Vec<UnitDeclaration>,
    package_path_mismatches: Vec<PackagePathMismatch>,
    diagnostics: Vec<Diagnostic>,
}
impl CompilationUnitIndex {
    pub(super) fn new(
        packages: Vec<Package>,
        source_units: Vec<SourceUnit>,
        declarations: Vec<UnitDeclaration>,
        package_path_mismatches: Vec<PackagePathMismatch>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            packages,
            source_units,
            declarations,
            package_path_mismatches,
            diagnostics,
        }
    }
    /// 返回按 package name 排序的 package 表。
    #[must_use]
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }
    /// 返回按稳定 source key 排序的 source-unit 表。
    #[must_use]
    pub fn source_units(&self) -> &[SourceUnit] {
        &self.source_units
    }
    /// 返回规范 source-unit/源码顺序下的顶层声明。
    #[must_use]
    pub fn declarations(&self) -> &[UnitDeclaration] {
        &self.declarations
    }
    /// 返回用于解释 L0146 的结构化事实。
    #[must_use]
    pub fn package_path_mismatches(&self) -> &[PackagePathMismatch] {
        &self.package_path_mismatches
    }
    /// 返回聚合 parser 与 unit 阶段后的稳定诊断全序。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 只证明 parser、package/path 和跨文件顶层冲突检查无 error；不代表 import/body 已解析。
    pub fn validate(self) -> Result<ValidatedCompilationUnitIndex, Box<Self>> {
        let has_errors = self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        if has_errors {
            Err(Box::new(self))
        } else {
            Ok(ValidatedCompilationUnitIndex(self))
        }
    }
}

/// 不可伪造的无错误 Stage 1 index；不得当作完整名称解析产物。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCompilationUnitIndex(CompilationUnitIndex);
impl ValidatedCompilationUnitIndex {
    /// 返回只读 Stage 1 index。
    #[must_use]
    pub const fn index(&self) -> &CompilationUnitIndex {
        &self.0
    }
    /// 解包为 recovery product。
    #[must_use]
    pub fn into_index(self) -> CompilationUnitIndex {
        self.0
    }
}
