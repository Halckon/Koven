use std::collections::BTreeMap;

use crate::{
    diagnostic::{Diagnostic, Severity},
    name_resolution::{DeclarationId, PackageId, UnitSymbolId},
    source::Span,
    type_checking::{
        BuiltinType, Capability, DeferredReason, IntrinsicTypeConstructor, NominalKind,
        ParameterMode,
    },
};

/// 一次 compilation-unit signature product 内的规范类型身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitTypeId(usize);

impl UnitTypeId {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    /// 返回 unit type table 中的稳定下标。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// unit function type 中的规范化参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitFunctionParameterType {
    mode: ParameterMode,
    ty: UnitTypeId,
}

impl UnitFunctionParameterType {
    pub(crate) const fn new(mode: ParameterMode, ty: UnitTypeId) -> Self {
        Self { mode, ty }
    }

    /// 返回参数传递模式。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回参数类型。
    #[must_use]
    pub const fn ty(self) -> UnitTypeId {
        self.ty
    }
}

/// unit-global type table 中的结构；源码身份始终带 source-unit 限定。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitTypeKind {
    /// 编译器内建类型。
    Builtin(BuiltinType),
    /// Nullable wrapper。
    Nullable(UnitTypeId),
    /// 函数类型。
    Function {
        /// 闭包值是否只能移动。
        move_only: bool,
        /// 源码顺序参数。
        parameters: Vec<UnitFunctionParameterType>,
        /// 返回类型。
        return_type: UnitTypeId,
    },
    /// 源码名义类型实例；顶层 identity 使用 DeclarationId。
    Nominal {
        /// classifier 的类型命名空间声明。
        declaration: DeclarationId,
        /// invariant 类型实参。
        arguments: Vec<UnitTypeId>,
    },
    /// 编译器拥有的 intrinsic 类型构造器实例。
    Intrinsic {
        /// 稳定 intrinsic identity。
        constructor: IntrinsicTypeConstructor,
        /// invariant 类型实参。
        arguments: Vec<UnitTypeId>,
    },
    /// 仅用于 type-test 的 enum case refinement。
    EnumCase {
        /// 带 source-unit 限定的 case type symbol。
        case: UnitSymbolId,
        /// 实例化后的 root enum runtime type。
        root: UnitTypeId,
    },
    /// source type parameter；UnitSymbolId 防止跨文件碰撞。
    TypeParameter(UnitSymbolId),
    /// interface body 中的静态 Self。
    StaticSelf(UnitTypeId),
    /// 编译器结构化能力。
    Capability(Capability),
    /// 后续 body 或候选能力才可决定的类型。
    Deferred(DeferredReason),
    /// 错误恢复类型。
    Error,
}

/// 按结构去重的唯一 compilation-unit 类型表。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitTypeTable {
    kinds: Vec<UnitTypeKind>,
    ids: BTreeMap<UnitTypeKind, UnitTypeId>,
}

impl UnitTypeTable {
    pub(crate) fn new() -> Self {
        let mut table = Self {
            kinds: Vec::new(),
            ids: BTreeMap::new(),
        };
        for builtin in BuiltinType::ALL {
            table.intern(UnitTypeKind::Builtin(builtin));
        }
        table.intern(UnitTypeKind::Error);
        table
    }

    pub(crate) fn intern(&mut self, kind: UnitTypeKind) -> UnitTypeId {
        if let Some(id) = self.ids.get(&kind).copied() {
            return id;
        }
        let id = UnitTypeId::new(self.kinds.len());
        self.kinds.push(kind.clone());
        self.ids.insert(kind, id);
        id
    }

    /// 按 identity 读取规范结构。
    #[must_use]
    pub fn get(&self, id: UnitTypeId) -> Option<&UnitTypeKind> {
        self.kinds.get(id.index())
    }

    /// 查询内建类型 identity。
    #[must_use]
    pub fn builtin(&self, builtin: BuiltinType) -> Option<UnitTypeId> {
        self.ids.get(&UnitTypeKind::Builtin(builtin)).copied()
    }

    /// 返回类型数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// 返回表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

/// unit callable 的静态 target；顶层与局部/member identity 不混用。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitCallableTarget {
    /// package 顶层函数 declaration。
    Declaration(DeclarationId),
    /// classifier member 或其他 source-local callable。
    Symbol(UnitSymbolId),
}

/// unit callable 的一个声明顺序参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCallableParameter {
    symbol: Option<UnitSymbolId>,
    name: Option<String>,
    mode: ParameterMode,
    ty: UnitTypeId,
    span: Span,
}

impl UnitCallableParameter {
    pub(crate) fn new(
        symbol: Option<UnitSymbolId>,
        name: Option<String>,
        mode: ParameterMode,
        ty: UnitTypeId,
        span: Span,
    ) -> Self {
        Self {
            symbol,
            name,
            mode,
            ty,
            span,
        }
    }

    /// 返回源码参数 symbol。
    #[must_use]
    pub const fn symbol(&self) -> Option<UnitSymbolId> {
        self.symbol
    }

    /// 返回参数名；恢复参数为 None。
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// 返回规范化参数模式。
    #[must_use]
    pub const fn mode(&self) -> ParameterMode {
        self.mode
    }

    /// 返回参数类型。
    #[must_use]
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }

    /// 返回参数声明范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// unit-wide 收集的 callable signature。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCallableSignature {
    target: UnitCallableTarget,
    name: String,
    name_span: Span,
    type_parameters: Vec<UnitSymbolId>,
    parameters: Vec<UnitCallableParameter>,
    return_type: UnitTypeId,
    callable_type: UnitTypeId,
}

impl UnitCallableSignature {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        target: UnitCallableTarget,
        name: String,
        name_span: Span,
        type_parameters: Vec<UnitSymbolId>,
        parameters: Vec<UnitCallableParameter>,
        return_type: UnitTypeId,
        callable_type: UnitTypeId,
    ) -> Self {
        Self {
            target,
            name,
            name_span,
            type_parameters,
            parameters,
            return_type,
            callable_type,
        }
    }

    /// 返回静态 target identity。
    #[must_use]
    pub const fn target(&self) -> UnitCallableTarget {
        self.target
    }

    /// 返回声明名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回名称范围。
    #[must_use]
    pub const fn name_span(&self) -> Span {
        self.name_span
    }

    /// 返回 callable 自身的类型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[UnitSymbolId] {
        &self.type_parameters
    }

    /// 返回声明顺序参数。
    #[must_use]
    pub fn parameters(&self) -> &[UnitCallableParameter] {
        &self.parameters
    }

    /// 返回返回类型。
    #[must_use]
    pub const fn return_type(&self) -> UnitTypeId {
        self.return_type
    }

    /// 返回完整函数类型。
    #[must_use]
    pub const fn callable_type(&self) -> UnitTypeId {
        self.callable_type
    }
}

/// 一个主构造器 field signature。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitFieldSignature {
    symbol: UnitSymbolId,
    name: String,
    ty: UnitTypeId,
    span: Span,
}

/// 一个 enum case 的 unit-global signature；case identity 由带 source 的 symbol 限定。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitEnumCaseSignature {
    value_symbol: UnitSymbolId,
    type_symbol: UnitSymbolId,
    name: String,
    name_span: Span,
    case_type: UnitTypeId,
    value_type: UnitTypeId,
    payloads: Vec<UnitFieldSignature>,
}

impl UnitEnumCaseSignature {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        value_symbol: UnitSymbolId,
        type_symbol: UnitSymbolId,
        name: String,
        name_span: Span,
        case_type: UnitTypeId,
        value_type: UnitTypeId,
        payloads: Vec<UnitFieldSignature>,
    ) -> Self {
        Self {
            value_symbol,
            type_symbol,
            name,
            name_span,
            case_type,
            value_type,
            payloads,
        }
    }

    /// 返回值命名空间 case/constructor symbol。
    #[must_use]
    pub const fn value_symbol(&self) -> UnitSymbolId {
        self.value_symbol
    }

    /// 返回 type-test 命名空间 case symbol。
    #[must_use]
    pub const fn type_symbol(&self) -> UnitSymbolId {
        self.type_symbol
    }

    /// 返回 case 名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回 case 名称范围。
    #[must_use]
    pub const fn name_span(&self) -> Span {
        self.name_span
    }

    /// 返回仅供 type-test 的 case refinement type。
    #[must_use]
    pub const fn case_type(&self) -> UnitTypeId {
        self.case_type
    }

    /// 返回零 payload root value 或 payload constructor function type。
    #[must_use]
    pub const fn value_type(&self) -> UnitTypeId {
        self.value_type
    }

    /// 返回声明顺序 payloads。
    #[must_use]
    pub fn payloads(&self) -> &[UnitFieldSignature] {
        &self.payloads
    }
}

impl UnitFieldSignature {
    pub(crate) fn new(symbol: UnitSymbolId, name: String, ty: UnitTypeId, span: Span) -> Self {
        Self {
            symbol,
            name,
            ty,
            span,
        }
    }

    /// 返回 field identity。
    #[must_use]
    pub const fn symbol(&self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回 field 名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回 field 类型。
    #[must_use]
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }

    /// 返回 field 名称范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// unit-global nominal signature。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNominalSignature {
    declaration: DeclarationId,
    symbol: UnitSymbolId,
    kind: NominalKind,
    ty: UnitTypeId,
    type_parameters: Vec<UnitSymbolId>,
    direct_interfaces: Vec<UnitTypeId>,
    fields: Vec<UnitFieldSignature>,
    enum_cases: Vec<UnitEnumCaseSignature>,
    members: Vec<UnitCallableSignature>,
}

impl UnitNominalSignature {
    pub(crate) fn new(
        declaration: DeclarationId,
        symbol: UnitSymbolId,
        kind: NominalKind,
        ty: UnitTypeId,
        type_parameters: Vec<UnitSymbolId>,
    ) -> Self {
        Self {
            declaration,
            symbol,
            kind,
            ty,
            type_parameters,
            direct_interfaces: Vec::new(),
            fields: Vec::new(),
            enum_cases: Vec::new(),
            members: Vec::new(),
        }
    }

    /// 返回顶层类型 declaration。
    #[must_use]
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    /// 返回 classifier unit symbol。
    #[must_use]
    pub const fn symbol(&self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回名义类别。
    #[must_use]
    pub const fn kind(&self) -> NominalKind {
        self.kind
    }

    /// 返回声明自身的泛型 nominal 类型。
    #[must_use]
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }

    /// 返回 classifier 类型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[UnitSymbolId] {
        &self.type_parameters
    }

    /// 返回已解析的直接 interface instances。
    #[must_use]
    pub fn direct_interfaces(&self) -> &[UnitTypeId] {
        &self.direct_interfaces
    }

    /// 返回主构造器 fields。
    #[must_use]
    pub fn fields(&self) -> &[UnitFieldSignature] {
        &self.fields
    }

    /// 返回 enum case signatures；其他 nominal 为空。
    #[must_use]
    pub fn enum_cases(&self) -> &[UnitEnumCaseSignature] {
        &self.enum_cases
    }

    /// 返回 instance/companion callable signatures。
    #[must_use]
    pub fn members(&self) -> &[UnitCallableSignature] {
        &self.members
    }

    pub(crate) fn set_direct_interfaces(&mut self, interfaces: Vec<UnitTypeId>) {
        self.direct_interfaces = interfaces;
    }

    pub(crate) fn set_fields(&mut self, fields: Vec<UnitFieldSignature>) {
        self.fields = fields;
    }

    pub(crate) fn set_enum_cases(&mut self, enum_cases: Vec<UnitEnumCaseSignature>) {
        self.enum_cases = enum_cases;
    }

    pub(crate) fn set_members(&mut self, members: Vec<UnitCallableSignature>) {
        self.members = members;
    }
}

/// 一个顶层 DeclarationId 的 signature/type 映射。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDeclarationSignature {
    declaration: DeclarationId,
    symbol: UnitSymbolId,
    package: PackageId,
    ty: UnitTypeId,
    callable: Option<UnitCallableSignature>,
    nominal: Option<UnitNominalSignature>,
}

impl UnitDeclarationSignature {
    pub(crate) fn new(
        declaration: DeclarationId,
        symbol: UnitSymbolId,
        package: PackageId,
        ty: UnitTypeId,
        callable: Option<UnitCallableSignature>,
        nominal: Option<UnitNominalSignature>,
    ) -> Self {
        Self {
            declaration,
            symbol,
            package,
            ty,
            callable,
            nominal,
        }
    }

    /// 返回 declaration identity。
    #[must_use]
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    /// 返回对应源码 unit symbol。
    #[must_use]
    pub const fn symbol(&self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回声明 package。
    #[must_use]
    pub const fn package(&self) -> PackageId {
        self.package
    }

    /// 返回声明类型。
    #[must_use]
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }

    /// 返回 callable signature（若适用）。
    #[must_use]
    pub const fn callable(&self) -> Option<&UnitCallableSignature> {
        self.callable.as_ref()
    }

    /// 返回 nominal signature（仅类型命名空间 classifier）。
    #[must_use]
    pub const fn nominal(&self) -> Option<&UnitNominalSignature> {
        self.nominal.as_ref()
    }
}

/// SPEC-0197 第 1 阶段的 recovery signature product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitSignatures {
    types: UnitTypeTable,
    declarations: Vec<UnitDeclarationSignature>,
    symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    diagnostics: Vec<Diagnostic>,
}

impl CompilationUnitSignatures {
    pub(crate) fn new(
        types: UnitTypeTable,
        declarations: Vec<UnitDeclarationSignature>,
        symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            types,
            declarations,
            symbol_types,
            diagnostics,
        }
    }

    /// 返回唯一 unit-global type table。
    #[must_use]
    pub const fn types(&self) -> &UnitTypeTable {
        &self.types
    }

    /// 返回 DeclarationId 顺序的 signatures。
    #[must_use]
    pub fn declarations(&self) -> &[UnitDeclarationSignature] {
        &self.declarations
    }

    /// 查询顶层 declaration signature。
    #[must_use]
    pub fn declaration(&self, id: DeclarationId) -> Option<&UnitDeclarationSignature> {
        self.declarations.get(id.index())
    }

    /// 查询任一 source unit symbol 的规范类型。
    #[must_use]
    pub fn symbol_type(&self, symbol: UnitSymbolId) -> Option<UnitTypeId> {
        self.symbol_types.get(&symbol).copied()
    }

    /// 返回仅属于类型签名阶段的稳定诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 仅无 signature error 时取得下一类型子阶段可消费的 view。
    pub fn validate(self) -> Result<ValidatedCompilationUnitSignatures, Box<Self>> {
        if self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            Err(Box::new(self))
        } else {
            Ok(ValidatedCompilationUnitSignatures(self))
        }
    }
}

/// 不可伪造的无错误 compilation-unit signature product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCompilationUnitSignatures(CompilationUnitSignatures);

impl ValidatedCompilationUnitSignatures {
    /// 返回 recovery product 的只读视图。
    #[must_use]
    pub const fn signatures(&self) -> &CompilationUnitSignatures {
        &self.0
    }

    /// 解包 recovery product。
    #[must_use]
    pub fn into_signatures(self) -> CompilationUnitSignatures {
        self.0
    }
}
