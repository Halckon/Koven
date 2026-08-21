use std::{collections::BTreeMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    source::{SourceId, Span},
};

macro_rules! define_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub(crate) usize);

        impl $name {
            /// 返回本次解析产物内的稳定下标。
            #[must_use]
            pub const fn index(self) -> usize {
                self.0
            }
        }
    };
}

define_id!(ScopeId, "词法作用域在单次名称解析产物中的身份。");
define_id!(SymbolId, "源码 symbol 在单次名称解析产物中的身份。");
define_id!(EnumCaseId, "enum case 在单次名称解析产物中的身份。");
define_id!(ExternalSymbolId, "显式名称环境中预声明 symbol 的身份。");

/// 名称所属的独立命名空间。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    /// classifier、类型参数和 TypeRef 首段。
    Type,
    /// 变量、常量、参数、函数和普通名称表达式。
    Value,
}

/// 外部环境中预声明名称的类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalSymbolKind {
    /// 预声明类型。
    Type,
    /// 预声明非函数值。
    Value,
    /// 可形成 overload set 的预声明函数。
    Function,
}

/// 一个显式外部预声明。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalSymbol {
    id: ExternalSymbolId,
    name: String,
    kind: ExternalSymbolKind,
}

impl ExternalSymbol {
    /// 返回环境内身份。
    #[must_use]
    pub const fn id(&self) -> ExternalSymbolId {
        self.id
    }
    /// 返回精确名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// 返回预声明类别。
    #[must_use]
    pub const fn kind(&self) -> ExternalSymbolKind {
        self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalBinding {
    Single(ExternalSymbolId),
    Functions(Vec<ExternalSymbolId>),
}

/// 显式传给名称解析器的不可变预声明集合。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEnvironment {
    owner: Arc<()>,
    symbols: Vec<ExternalSymbol>,
    types: BTreeMap<String, ExternalBinding>,
    values: BTreeMap<String, ExternalBinding>,
}

impl Default for NameEnvironment {
    fn default() -> Self {
        Self {
            owner: Arc::new(()),
            symbols: Vec::new(),
            types: BTreeMap::new(),
            values: BTreeMap::new(),
        }
    }
}

/// 外部环境声明冲突。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEnvironmentError {
    name: String,
    namespace: Namespace,
}

impl std::fmt::Display for NameEnvironmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "external name {:?} conflicts in the {:?} namespace",
            self.name, self.namespace
        )
    }
}
impl std::error::Error for NameEnvironmentError {}

impl NameEnvironment {
    /// 创建空环境。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 声明外部类型。
    pub fn declare_type(
        &mut self,
        name: impl Into<String>,
    ) -> Result<ExternalSymbolId, NameEnvironmentError> {
        self.declare(name.into(), ExternalSymbolKind::Type)
    }
    /// 声明外部非函数值。
    pub fn declare_value(
        &mut self,
        name: impl Into<String>,
    ) -> Result<ExternalSymbolId, NameEnvironmentError> {
        self.declare(name.into(), ExternalSymbolKind::Value)
    }
    /// 声明外部函数；同名函数按插入顺序形成 overload set。
    pub fn declare_function(
        &mut self,
        name: impl Into<String>,
    ) -> Result<ExternalSymbolId, NameEnvironmentError> {
        self.declare(name.into(), ExternalSymbolKind::Function)
    }
    /// 返回按声明顺序排列的外部 symbol。
    #[must_use]
    pub fn symbols(&self) -> &[ExternalSymbol] {
        &self.symbols
    }

    /// 按环境内身份读取外部 symbol。
    #[must_use]
    pub fn symbol(&self, id: ExternalSymbolId) -> Option<&ExternalSymbol> {
        self.symbols.get(id.index())
    }

    pub(crate) fn owner(&self) -> Arc<()> {
        Arc::clone(&self.owner)
    }

    fn declare(
        &mut self,
        name: String,
        kind: ExternalSymbolKind,
    ) -> Result<ExternalSymbolId, NameEnvironmentError> {
        let namespace = if kind == ExternalSymbolKind::Type {
            Namespace::Type
        } else {
            Namespace::Value
        };
        let table = if namespace == Namespace::Type {
            &mut self.types
        } else {
            &mut self.values
        };
        if let Some(existing) = table.get_mut(&name) {
            if kind == ExternalSymbolKind::Function
                && let ExternalBinding::Functions(ids) = existing
            {
                let id = ExternalSymbolId(self.symbols.len());
                self.symbols.push(ExternalSymbol { id, name, kind });
                ids.push(id);
                return Ok(id);
            }
            return Err(NameEnvironmentError { name, namespace });
        }
        let id = ExternalSymbolId(self.symbols.len());
        self.symbols.push(ExternalSymbol {
            id,
            name: name.clone(),
            kind,
        });
        let binding = if kind == ExternalSymbolKind::Function {
            ExternalBinding::Functions(vec![id])
        } else {
            ExternalBinding::Single(id)
        };
        table.insert(name, binding);
        Ok(id)
    }

    pub(crate) fn lookup(&self, namespace: Namespace, name: &str) -> Option<&ExternalBinding> {
        match namespace {
            Namespace::Type => self.types.get(name),
            Namespace::Value => self.values.get(name),
        }
    }
}

/// 词法作用域类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    /// 文件根作用域。
    File,
    /// classifier 实例成员作用域。
    Classifier,
    /// companion 类型级作用域。
    Companion,
    /// 具名函数作用域。
    Function,
    /// lambda 参数与 body 作用域。
    Lambda,
    /// 普通 block 作用域。
    Block,
    /// if/when 分支 body 作用域。
    ControlBody,
    /// loop/for binding 作用域。
    Loop,
    /// enum 变体关联数据参数作用域。
    EnumVariant,
}

/// 一个公开只读词法作用域。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope {
    id: ScopeId,
    parent: Option<ScopeId>,
    kind: ScopeKind,
    span: Option<Span>,
}
impl Scope {
    /// 返回作用域身份。
    #[must_use]
    pub const fn id(&self) -> ScopeId {
        self.id
    }
    /// 返回词法父作用域。
    #[must_use]
    pub const fn parent(&self) -> Option<ScopeId> {
        self.parent
    }
    /// 返回作用域类别。
    #[must_use]
    pub const fn kind(&self) -> ScopeKind {
        self.kind
    }
    /// 返回拥有该作用域的源码范围。
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
    pub(crate) const fn new(
        id: ScopeId,
        parent: Option<ScopeId>,
        kind: ScopeKind,
        span: Option<Span>,
    ) -> Self {
        Self {
            id,
            parent,
            kind,
            span,
        }
    }
}

/// 源码 symbol 类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    /// classifier 类型声明。
    Classifier,
    /// 具名 object 的 singleton value。
    ObjectValue,
    /// 泛型类型参数。
    TypeParameter,
    /// val/var 声明。
    Variable,
    /// const val 声明。
    Constant,
    /// 具名函数。
    Function,
    /// 主构造器字段。
    Field,
    /// enum 变体。
    EnumVariant,
    /// 仅供 type-test 使用的 enum case type。
    EnumCaseType,
    /// 具名函数或变体值参数。
    ValueParameter,
    /// lambda 参数。
    LambdaParameter,
    /// for binding。
    ForBinding,
    /// 局部解构 binding。
    DestructuringBinding,
}

/// 同时关联值构造器、type-test 身份与 payload 参数的 enum case。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumCase {
    id: EnumCaseId,
    root: SymbolId,
    value_symbol: SymbolId,
    type_symbol: SymbolId,
    span: Span,
    payloads: Vec<SymbolId>,
}

impl EnumCase {
    /// 返回源码顺序的 case 身份。
    #[must_use]
    pub const fn id(&self) -> EnumCaseId {
        self.id
    }
    /// 返回所属 root enum classifier symbol。
    #[must_use]
    pub const fn root(&self) -> SymbolId {
        self.root
    }
    /// 返回值命名空间中的 case value/constructor symbol。
    #[must_use]
    pub const fn value_symbol(&self) -> SymbolId {
        self.value_symbol
    }
    /// 返回类型命名空间中的 case type symbol。
    #[must_use]
    pub const fn type_symbol(&self) -> SymbolId {
        self.type_symbol
    }
    /// 返回完整 case 声明范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// 返回源码顺序的 payload 参数 symbol。
    #[must_use]
    pub fn payloads(&self) -> &[SymbolId] {
        &self.payloads
    }
    pub(crate) const fn new(
        id: EnumCaseId,
        root: SymbolId,
        value_symbol: SymbolId,
        type_symbol: SymbolId,
        span: Span,
    ) -> Self {
        Self {
            id,
            root,
            value_symbol,
            type_symbol,
            span,
            payloads: Vec::new(),
        }
    }
    pub(crate) fn set_payloads(&mut self, payloads: Vec<SymbolId>) {
        self.payloads = payloads;
    }
}

/// 一个已收集源码 symbol。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    id: SymbolId,
    name: String,
    span: Span,
    scope: ScopeId,
    namespace: Namespace,
    kind: SymbolKind,
}
impl Symbol {
    /// 返回 symbol 身份。
    #[must_use]
    pub const fn id(&self) -> SymbolId {
        self.id
    }
    /// 返回精确源码名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// 返回声明名称范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// 返回所属作用域。
    #[must_use]
    pub const fn scope(&self) -> ScopeId {
        self.scope
    }
    /// 返回所属命名空间。
    #[must_use]
    pub const fn namespace(&self) -> Namespace {
        self.namespace
    }
    /// 返回 symbol 类别。
    #[must_use]
    pub const fn kind(&self) -> SymbolKind {
        self.kind
    }
    pub(crate) fn new(
        id: SymbolId,
        name: String,
        span: Span,
        scope: ScopeId,
        namespace: Namespace,
        kind: SymbolKind,
    ) -> Self {
        Self {
            id,
            name,
            span,
            scope,
            namespace,
            kind,
        }
    }
}

/// 名称引用的解析结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceTarget {
    /// 唯一源码 symbol。
    Symbol(SymbolId),
    /// 源码有序函数 overload set。
    OverloadSet(Vec<SymbolId>),
    /// 唯一外部 symbol。
    External(ExternalSymbolId),
    /// 外部环境中的有序函数 overload set。
    ExternalOverloadSet(Vec<ExternalSymbolId>),
    /// 等待 smart-cast facts 唯一选择的 enum case payload symbols。
    EnumCasePayloadCandidates(Vec<SymbolId>),
    /// 当前环境中未解析。
    Unresolved,
    /// 同一顺序作用域稍后出现的 local 声明范围。
    LaterLocal(Span),
}

/// AST 中一个需要名称解析的真实 Identifier。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameReference {
    span: Span,
    scope: ScopeId,
    namespace: Namespace,
    target: ReferenceTarget,
}
impl NameReference {
    /// 返回引用 Identifier 范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// 返回引用发生的作用域。
    #[must_use]
    pub const fn scope(&self) -> ScopeId {
        self.scope
    }
    /// 返回查询的命名空间。
    #[must_use]
    pub const fn namespace(&self) -> Namespace {
        self.namespace
    }
    /// 返回解析目标。
    #[must_use]
    pub const fn target(&self) -> &ReferenceTarget {
        &self.target
    }
    pub(crate) const fn new(
        span: Span,
        scope: ScopeId,
        namespace: Namespace,
        target: ReferenceTarget,
    ) -> Self {
        Self {
            span,
            scope,
            namespace,
            target,
        }
    }
}

/// 一份单文件名称解析产物。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameResolution {
    source_id: SourceId,
    environment_owner: Arc<()>,
    scopes: Vec<Scope>,
    symbols: Vec<Symbol>,
    enum_cases: Vec<EnumCase>,
    references: Vec<NameReference>,
    diagnostics: Vec<Diagnostic>,
}
impl NameResolution {
    pub(crate) fn new(
        source_id: SourceId,
        environment_owner: Arc<()>,
        scopes: Vec<Scope>,
        symbols: Vec<Symbol>,
        enum_cases: Vec<EnumCase>,
        references: Vec<NameReference>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            source_id,
            environment_owner,
            scopes,
            symbols,
            enum_cases,
            references,
            diagnostics,
        }
    }
    /// 返回输入源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }
    /// 返回按分配顺序排列的作用域。
    #[must_use]
    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }
    /// 返回按分配顺序排列的源码 symbol。
    #[must_use]
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }
    /// 返回源码顺序的 enum case 身份与双命名空间关联。
    #[must_use]
    pub fn enum_cases(&self) -> &[EnumCase] {
        &self.enum_cases
    }
    /// 返回按 AST 遍历顺序记录的名称引用。
    #[must_use]
    pub fn references(&self) -> &[NameReference] {
        &self.references
    }
    /// 返回确定性全序下的名称诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub(crate) fn environment_owner(&self) -> &Arc<()> {
        &self.environment_owner
    }
}
