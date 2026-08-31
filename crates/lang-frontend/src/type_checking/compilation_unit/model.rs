use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, Severity},
    name_resolution::{
        CompilationUnitIndex, DeclarationId, DeclarationVisibility, PackageId, SourceUnitInput,
        UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, Capability, DeferredReason, IntegerConstraint, IntrinsicTypeConstructor,
        NominalKind, ParameterMode, TypeEnvironment,
        canonical::{CanonicalTypeId, CanonicalTypeKind, CanonicalTypeTable},
    },
};

macro_rules! define_unit_ast_id {
    ($name:ident, $local:ty, $field:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name {
            source_unit: crate::name_resolution::SourceUnitId,
            $field: $local,
        }

        impl $name {
            /// 从 source-unit 与文件局部 AST identity 构造不可碰撞的 unit identity。
            #[must_use]
            pub const fn new(
                source_unit: crate::name_resolution::SourceUnitId,
                $field: $local,
            ) -> Self {
                Self {
                    source_unit,
                    $field,
                }
            }

            /// 返回 AST 节点所属的 source unit。
            #[must_use]
            pub const fn source_unit(self) -> crate::name_resolution::SourceUnitId {
                self.source_unit
            }

            /// 返回所属文件内的局部 AST identity。
            #[must_use]
            pub const fn $field(self) -> $local {
                self.$field
            }
        }

        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $name {
            fn cmp(&self, other: &Self) -> Ordering {
                (self.source_unit, self.$field.index())
                    .cmp(&(other.source_unit, other.$field.index()))
            }
        }

        impl Hash for $name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.source_unit.hash(state);
                self.$field.index().hash(state);
            }
        }
    };
}

define_unit_ast_id!(
    UnitExpressionId,
    ExpressionId,
    expression,
    "compilation unit 中一个带 source-unit 限定的 expression identity。"
);
define_unit_ast_id!(
    UnitItemId,
    ItemId,
    item,
    "compilation unit 中一个带 source-unit 限定的 item identity。"
);
define_unit_ast_id!(
    UnitStatementId,
    StatementId,
    statement,
    "compilation unit 中一个带 source-unit 限定的 statement identity。"
);
define_unit_ast_id!(
    UnitTypeRefId,
    TypeRefId,
    type_ref,
    "compilation unit 中一个带 source-unit 限定的 type-reference identity。"
);

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

impl CanonicalTypeId for UnitTypeId {
    fn from_index(index: usize) -> Self {
        Self::new(index)
    }

    fn index(self) -> usize {
        self.index()
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
    /// 整数字面量在 expected-type 定型前的约束族。
    IntegerLiteral(IntegerConstraint),
    /// 后续 body 或候选能力才可决定的类型。
    Deferred(DeferredReason),
    /// 错误恢复类型。
    Error,
}

impl CanonicalTypeKind for UnitTypeKind {
    type Id = UnitTypeId;

    fn initial_kinds() -> Vec<Self> {
        BuiltinType::ALL
            .into_iter()
            .map(Self::Builtin)
            .chain([
                Self::IntegerLiteral(IntegerConstraint::Signed),
                Self::IntegerLiteral(IntegerConstraint::Unsigned),
                Self::Error,
            ])
            .collect()
    }

    fn builtin(builtin: BuiltinType) -> Self {
        Self::Builtin(builtin)
    }
}

/// 按结构去重的唯一 compilation-unit 类型表。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitTypeTable {
    canonical: CanonicalTypeTable<UnitTypeKind>,
}

impl UnitTypeTable {
    pub(crate) fn new() -> Self {
        Self {
            canonical: CanonicalTypeTable::new(),
        }
    }

    pub(crate) fn intern(&mut self, kind: UnitTypeKind) -> UnitTypeId {
        self.canonical.intern(kind)
    }

    /// 按 identity 读取规范结构。
    #[must_use]
    pub fn get(&self, id: UnitTypeId) -> Option<&UnitTypeKind> {
        self.canonical.get(id)
    }

    /// 查询内建类型 identity。
    #[must_use]
    pub fn builtin(&self, builtin: BuiltinType) -> Option<UnitTypeId> {
        self.canonical.builtin(builtin)
    }

    /// 查询一个已经由 validated compilation-unit product 规范化的类型 identity。
    #[must_use]
    pub fn find(&self, kind: &UnitTypeKind) -> Option<UnitTypeId> {
        self.canonical.find(kind)
    }

    /// 返回类型数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.canonical.len()
    }

    /// 返回表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty()
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
    receiver: Option<UnitCallableReceiver>,
    parameters: Vec<UnitCallableParameter>,
    return_type: UnitTypeId,
    callable_type: UnitTypeId,
    visibility: DeclarationVisibility,
    has_body: bool,
}

/// compilation-unit instance callable 的隐藏 receiver 契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCallableReceiver {
    mode: ParameterMode,
    ty: UnitTypeId,
    declaration_span: Span,
    marker_span: Option<Span>,
}

impl UnitCallableReceiver {
    pub(crate) const fn new(
        mode: ParameterMode,
        ty: UnitTypeId,
        declaration_span: Span,
        marker_span: Option<Span>,
    ) -> Self {
        Self {
            mode,
            ty,
            declaration_span,
            marker_span,
        }
    }

    /// 返回规范化 receiver mode。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回 owner 类型参数尚未实例化时的 receiver 类型模板。
    #[must_use]
    pub const fn ty(self) -> UnitTypeId {
        self.ty
    }

    /// 返回 callable 声明名范围。
    #[must_use]
    pub const fn declaration_span(self) -> Span {
        self.declaration_span
    }

    /// 返回显式 marker 范围；缺省 Borrow 为 `None`。
    #[must_use]
    pub const fn marker_span(self) -> Option<Span> {
        self.marker_span
    }
}

/// compilation-unit 类型参数的规范化上界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitTypeParameterBound {
    /// 缺省或显式 `Any` 上界。
    Any,
    /// 静态 interface instance 上界。
    Interface(UnitTypeId),
    /// 编译器封闭能力上界。
    Capability(Capability),
    /// 无效上界的恢复状态。
    Error,
}

/// 一个 source-local 类型参数在 unit 类型空间中的描述。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitTypeParameterDescriptor {
    symbol: UnitSymbolId,
    bound: UnitTypeParameterBound,
}

impl UnitTypeParameterDescriptor {
    pub(crate) const fn new(symbol: UnitSymbolId) -> Self {
        Self {
            symbol,
            bound: UnitTypeParameterBound::Any,
        }
    }

    /// 返回带 source-unit 限定的参数 identity。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回规范化静态上界。
    #[must_use]
    pub const fn bound(self) -> UnitTypeParameterBound {
        self.bound
    }

    pub(crate) fn set_bound(&mut self, bound: UnitTypeParameterBound) {
        self.bound = bound;
    }
}

impl UnitCallableSignature {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        target: UnitCallableTarget,
        name: String,
        name_span: Span,
        type_parameters: Vec<UnitSymbolId>,
        receiver: Option<UnitCallableReceiver>,
        parameters: Vec<UnitCallableParameter>,
        return_type: UnitTypeId,
        callable_type: UnitTypeId,
        visibility: DeclarationVisibility,
        has_body: bool,
    ) -> Self {
        Self {
            target,
            name,
            name_span,
            type_parameters,
            receiver,
            parameters,
            return_type,
            callable_type,
            visibility,
            has_body,
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

    /// 返回 instance member 的 receiver 契约；顶层/companion callable 为 `None`。
    #[must_use]
    pub const fn receiver(&self) -> Option<UnitCallableReceiver> {
        self.receiver
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

    /// 返回 callable 的规范化可见性。
    #[must_use]
    pub const fn visibility(&self) -> DeclarationVisibility {
        self.visibility
    }

    /// 返回 callable 是否具有源码 body。
    #[must_use]
    pub const fn has_body(&self) -> bool {
        self.has_body
    }
}

/// 一个主构造器 field signature。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitFieldSignature {
    symbol: UnitSymbolId,
    name: String,
    ty: UnitTypeId,
    span: Span,
    visibility: DeclarationVisibility,
}

/// 一个已验证、可供后续 body 与 lowering 消费的 interface 委托计划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDelegationPlan {
    owner: DeclarationId,
    interface: UnitTypeId,
    target: UnitSymbolId,
    delegation_span: Span,
    by_span: Span,
    forwarders: Vec<UnitDelegationForwarderDescriptor>,
}

/// 一个 Borrow-only interface requirement 的静态 delegate forwarder。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDelegationForwarderDescriptor {
    requirement: UnitCallableTarget,
    receiver_type: UnitTypeId,
    resolution: UnitDelegationForwarderResolution,
    type_parameters: Vec<UnitSymbolId>,
    parameters: Vec<UnitCallableParameter>,
    return_type: UnitTypeId,
    declaration_span: Span,
}

impl UnitDelegationForwarderDescriptor {
    pub(crate) fn new(
        requirement: UnitCallableTarget,
        receiver_type: UnitTypeId,
        implementation: Option<UnitDelegationImplementationDescriptor>,
        type_parameters: Vec<UnitSymbolId>,
        parameters: Vec<UnitCallableParameter>,
        return_type: UnitTypeId,
        declaration_span: Span,
    ) -> Self {
        Self {
            requirement,
            receiver_type,
            resolution: implementation.map_or(
                UnitDelegationForwarderResolution::Unresolved,
                UnitDelegationForwarderResolution::Implementation,
            ),
            type_parameters,
            parameters,
            return_type,
            declaration_span,
        }
    }

    /// 返回被转发的 interface member target。
    #[must_use]
    pub const fn requirement(&self) -> UnitCallableTarget {
        self.requirement
    }

    /// 返回完整 interface instance receiver 类型。
    #[must_use]
    pub const fn receiver_type(&self) -> UnitTypeId {
        self.receiver_type
    }

    /// 返回 delegate concrete type 上可直接调用的已验证有体实现。
    ///
    /// `None` 表示该 forwarder 是 exact next hop 或仍需具体单态化/实例化才能解析。
    #[must_use]
    pub const fn implementation(&self) -> Option<UnitDelegationImplementationDescriptor> {
        match self.resolution {
            UnitDelegationForwarderResolution::Implementation(implementation) => {
                Some(implementation)
            }
            UnitDelegationForwarderResolution::NextHop(_)
            | UnitDelegationForwarderResolution::Unresolved => None,
        }
    }

    /// 返回 delegate 自身 delegation plan 中已验证的精确下一跳。
    #[must_use]
    pub const fn next_hop(&self) -> Option<UnitDelegationNextHopDescriptor> {
        match self.resolution {
            UnitDelegationForwarderResolution::NextHop(next_hop) => Some(next_hop),
            UnitDelegationForwarderResolution::Implementation(_)
            | UnitDelegationForwarderResolution::Unresolved => None,
        }
    }

    pub(crate) const fn set_next_hop(&mut self, next_hop: UnitDelegationNextHopDescriptor) {
        self.resolution = UnitDelegationForwarderResolution::NextHop(next_hop);
    }

    pub(crate) const fn set_unresolved(&mut self) {
        self.resolution = UnitDelegationForwarderResolution::Unresolved;
    }

    /// delegate forwarder receiver 固定为 Borrow。
    #[must_use]
    pub const fn receiver_mode(&self) -> ParameterMode {
        ParameterMode::Borrow
    }

    /// 返回转发 callable 的源码顺序泛型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[UnitSymbolId] {
        &self.type_parameters
    }

    /// 返回转发 callable 的显式参数契约。
    #[must_use]
    pub fn parameters(&self) -> &[UnitCallableParameter] {
        &self.parameters
    }

    /// 返回转发 callable 的实例化返回类型。
    #[must_use]
    pub const fn return_type(&self) -> UnitTypeId {
        self.return_type
    }

    /// 返回 interface requirement 声明范围。
    #[must_use]
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitDelegationForwarderResolution {
    Implementation(UnitDelegationImplementationDescriptor),
    NextHop(UnitDelegationNextHopDescriptor),
    Unresolved,
}

/// 一个 delegate forwarder 已解析出的直接有体实现及其 receiver template。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDelegationImplementationDescriptor {
    target: UnitCallableTarget,
    receiver_type: UnitTypeId,
}

impl UnitDelegationImplementationDescriptor {
    pub(crate) const fn new(target: UnitCallableTarget, receiver_type: UnitTypeId) -> Self {
        Self {
            target,
            receiver_type,
        }
    }

    /// 返回可直接调用的有体实现 identity。
    #[must_use]
    pub const fn target(self) -> UnitCallableTarget {
        self.target
    }

    /// 返回实现所属的完整 receiver type template。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }
}

/// 一个 delegate forwarder 已解析出的 delegation plan 下一跳。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDelegationNextHopDescriptor {
    requirement: UnitCallableTarget,
    receiver_type: UnitTypeId,
}

impl UnitDelegationNextHopDescriptor {
    pub(crate) const fn new(requirement: UnitCallableTarget, receiver_type: UnitTypeId) -> Self {
        Self {
            requirement,
            receiver_type,
        }
    }

    /// 返回下一跳 forwarder 的精确 requirement identity。
    #[must_use]
    pub const fn requirement(self) -> UnitCallableTarget {
        self.requirement
    }

    /// 返回下一跳 requirement 所属的完整 receiver type template。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }
}

impl UnitDelegationPlan {
    pub(crate) const fn new(
        owner: DeclarationId,
        interface: UnitTypeId,
        target: UnitSymbolId,
        delegation_span: Span,
        by_span: Span,
    ) -> Self {
        Self {
            owner,
            interface,
            target,
            delegation_span,
            by_span,
            forwarders: Vec::new(),
        }
    }

    /// 返回声明委托的 concrete classifier。
    #[must_use]
    pub const fn owner(&self) -> DeclarationId {
        self.owner
    }

    /// 返回包含完整类型实参的目标 interface instance。
    #[must_use]
    pub const fn interface(&self) -> UnitTypeId {
        self.interface
    }

    /// 返回同一主构造器中的 immutable delegate field。
    #[must_use]
    pub const fn target(&self) -> UnitSymbolId {
        self.target
    }

    /// 返回从 `by` 到 target 的完整 delegation clause 范围。
    #[must_use]
    pub const fn delegation_span(&self) -> Span {
        self.delegation_span
    }

    /// 返回真实 `by` token 范围。
    #[must_use]
    pub const fn by_span(&self) -> Span {
        self.by_span
    }

    /// 返回源码顺序的 Borrow-only delegate forwarder。
    #[must_use]
    pub fn forwarders(&self) -> &[UnitDelegationForwarderDescriptor] {
        &self.forwarders
    }

    pub(crate) fn forwarders_mut(&mut self) -> &mut [UnitDelegationForwarderDescriptor] {
        &mut self.forwarders
    }

    pub(crate) fn push_forwarder(&mut self, forwarder: UnitDelegationForwarderDescriptor) {
        self.forwarders.push(forwarder);
    }

    pub(crate) fn clear_forwarders(&mut self) {
        self.forwarders.clear();
    }

    pub(crate) fn sort_forwarders(&mut self) {
        self.forwarders.sort_by_key(|forwarder| {
            let symbol = match forwarder.requirement {
                UnitCallableTarget::Symbol(symbol) => symbol,
                UnitCallableTarget::Declaration(_) => {
                    unreachable!("delegate requirements are instance member symbols")
                }
            };
            (
                symbol.source_unit().index(),
                forwarder.declaration_span.start(),
            )
        });
    }
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
    pub(crate) fn new(
        symbol: UnitSymbolId,
        name: String,
        ty: UnitTypeId,
        span: Span,
        visibility: DeclarationVisibility,
    ) -> Self {
        Self {
            symbol,
            name,
            ty,
            span,
            visibility,
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

    /// 返回 field 的规范化可见性。
    #[must_use]
    pub const fn visibility(&self) -> DeclarationVisibility {
        self.visibility
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
    interfaces: Vec<UnitTypeId>,
    fields: Vec<UnitFieldSignature>,
    enum_cases: Vec<UnitEnumCaseSignature>,
    members: Vec<UnitCallableSignature>,
    companion_members: Vec<UnitCallableSignature>,
    static_dispatch_overrides: Vec<UnitStaticDispatchOverride>,
}

/// concrete classifier 对一个非委托 abstract requirement 的静态 effective implementation。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitStaticDispatchOverride {
    requirement: UnitCallableTarget,
    requirement_owner: UnitTypeId,
    implementation: UnitCallableTarget,
    implementation_owner: UnitTypeId,
}

impl UnitStaticDispatchOverride {
    pub(crate) const fn new(
        requirement: UnitCallableTarget,
        requirement_owner: UnitTypeId,
        implementation: UnitCallableTarget,
        implementation_owner: UnitTypeId,
    ) -> Self {
        Self {
            requirement,
            requirement_owner,
            implementation,
            implementation_owner,
        }
    }

    /// 返回 interface abstract requirement identity。
    #[must_use]
    pub const fn requirement(self) -> UnitCallableTarget {
        self.requirement
    }

    /// 返回 requirement 所属的 owner type template。
    #[must_use]
    pub const fn requirement_owner(self) -> UnitTypeId {
        self.requirement_owner
    }

    /// 返回已验证的本地 override 或唯一 inherited default identity。
    #[must_use]
    pub const fn implementation(self) -> UnitCallableTarget {
        self.implementation
    }

    /// 返回 implementation 所属的 owner type template。
    #[must_use]
    pub const fn implementation_owner(self) -> UnitTypeId {
        self.implementation_owner
    }
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
            interfaces: Vec::new(),
            fields: Vec::new(),
            enum_cases: Vec::new(),
            members: Vec::new(),
            companion_members: Vec::new(),
            static_dispatch_overrides: Vec::new(),
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

    /// 返回完成替换和去重后的传递 interface closure。
    #[must_use]
    pub fn interfaces(&self) -> &[UnitTypeId] {
        &self.interfaces
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

    /// 返回 instance callable signatures。
    #[must_use]
    pub fn members(&self) -> &[UnitCallableSignature] {
        &self.members
    }

    /// 返回 companion object callable signatures。
    #[must_use]
    pub fn companion_members(&self) -> &[UnitCallableSignature] {
        &self.companion_members
    }

    /// 返回非委托 abstract requirement 到 effective implementation 的确定映射。
    #[must_use]
    pub fn static_dispatch_overrides(&self) -> &[UnitStaticDispatchOverride] {
        &self.static_dispatch_overrides
    }

    pub(crate) fn set_direct_interfaces(&mut self, interfaces: Vec<UnitTypeId>) {
        self.direct_interfaces = interfaces;
    }

    pub(crate) fn set_interfaces(&mut self, interfaces: Vec<UnitTypeId>) {
        self.interfaces = interfaces;
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

    pub(crate) fn set_companion_members(&mut self, members: Vec<UnitCallableSignature>) {
        self.companion_members = members;
    }

    pub(crate) fn set_static_dispatch_overrides(
        &mut self,
        overrides: Vec<UnitStaticDispatchOverride>,
    ) {
        self.static_dispatch_overrides = overrides;
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
    provenance: SignatureProvenance,
    types: UnitTypeTable,
    declarations: Vec<UnitDeclarationSignature>,
    symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    type_ref_types: BTreeMap<UnitTypeRefId, UnitTypeId>,
    type_parameters: BTreeMap<UnitSymbolId, UnitTypeParameterDescriptor>,
    invalid_inline_nominals: BTreeSet<DeclarationId>,
    delegations: Vec<UnitDelegationPlan>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompilationUnitSignatureFacts {
    pub(crate) declarations: Vec<UnitDeclarationSignature>,
    pub(crate) symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    pub(crate) type_ref_types: BTreeMap<UnitTypeRefId, UnitTypeId>,
    pub(crate) type_parameters: BTreeMap<UnitSymbolId, UnitTypeParameterDescriptor>,
    pub(crate) invalid_inline_nominals: BTreeSet<DeclarationId>,
    pub(crate) delegations: Vec<UnitDelegationPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignatureProvenance {
    input_index: CompilationUnitIndex,
    environment_owner: Arc<()>,
    name_analysis_owners: Vec<Arc<()>>,
    analysis_owner: Arc<()>,
}

impl SignatureProvenance {
    pub(crate) fn new(
        input_index: CompilationUnitIndex,
        environment_owner: Arc<()>,
        name_analysis_owners: Vec<Arc<()>>,
    ) -> Self {
        Self {
            input_index,
            environment_owner,
            name_analysis_owners,
            analysis_owner: Arc::new(()),
        }
    }
}

impl CompilationUnitSignatures {
    pub(crate) fn new(
        provenance: SignatureProvenance,
        types: UnitTypeTable,
        facts: CompilationUnitSignatureFacts,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            provenance,
            types,
            declarations: facts.declarations,
            symbol_types: facts.symbol_types,
            type_ref_types: facts.type_ref_types,
            type_parameters: facts.type_parameters,
            invalid_inline_nominals: facts.invalid_inline_nominals,
            delegations: facts.delegations,
            diagnostics,
        }
    }

    /// 检查本签名产物是否来自给定 inputs、名称分析与类型环境身份链。
    #[must_use]
    pub fn is_compatible_with(
        &self,
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
        names: &ValidatedCompilationUnitNames,
        environment: &TypeEnvironment,
    ) -> bool {
        let unit_names = names.names();
        index_compilation_unit(sources, inputs)
            .is_ok_and(|index| index == self.provenance.input_index)
            && unit_names.index() == &self.provenance.input_index
            && Arc::ptr_eq(&self.provenance.environment_owner, environment.owner())
            && self.provenance.name_analysis_owners.len() == unit_names.source_units().len()
            && self
                .provenance
                .name_analysis_owners
                .iter()
                .zip(unit_names.source_units())
                .all(|(owner, source)| Arc::ptr_eq(owner, source.resolution().analysis_owner()))
    }

    /// 判断两个签名产物是否来自同一次签名分析；克隆产物保持该身份。
    #[must_use]
    pub fn is_same_analysis(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.provenance.analysis_owner,
            &other.provenance.analysis_owner,
        )
    }

    /// 返回唯一 unit-global type table。
    #[must_use]
    pub const fn types(&self) -> &UnitTypeTable {
        &self.types
    }

    pub(crate) fn types_mut(&mut self) -> &mut UnitTypeTable {
        &mut self.types
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

    /// 查询签名收集阶段已经解析的 source-qualified type reference。
    #[must_use]
    pub fn type_ref_type(&self, type_ref: UnitTypeRefId) -> Option<UnitTypeId> {
        self.type_ref_types.get(&type_ref).copied()
    }

    /// 返回签名收集阶段发布的 source-qualified type-reference facts。
    #[must_use]
    pub const fn type_ref_types(&self) -> &BTreeMap<UnitTypeRefId, UnitTypeId> {
        &self.type_ref_types
    }

    /// 返回 source-qualified 类型参数描述表。
    #[must_use]
    pub const fn type_parameters(&self) -> &BTreeMap<UnitSymbolId, UnitTypeParameterDescriptor> {
        &self.type_parameters
    }

    /// 查询一个类型参数的规范化描述。
    #[must_use]
    pub fn type_parameter(&self, symbol: UnitSymbolId) -> Option<UnitTypeParameterDescriptor> {
        self.type_parameters.get(&symbol).copied()
    }

    /// 返回签名阶段已诊断为无限内联布局的 nominal declarations。
    #[must_use]
    pub const fn invalid_inline_nominals(&self) -> &BTreeSet<DeclarationId> {
        &self.invalid_inline_nominals
    }

    /// 返回源码顺序的合法 interface 委托计划。
    #[must_use]
    pub fn delegations(&self) -> &[UnitDelegationPlan] {
        &self.delegations
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
