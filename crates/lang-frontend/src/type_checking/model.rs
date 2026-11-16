mod callable_descriptor;
mod type_table;
mod typed_file;
use crate::type_checking::{IntegerOperationDescriptor, IntegerRangeOperationDescriptor};
pub use callable_descriptor::CallableDescriptor;
use std::{collections::BTreeMap, sync::Arc};
pub use type_table::{DeferredReason, IntegerConstraint, TypeId, TypeKind, TypeTable};

use crate::{
    ast::{ExpressionId, StatementId, TypeRefId},
    diagnostic::Diagnostic,
    name_resolution::{
        EnumCaseId, ExternalSymbolId, ExternalSymbolKind, NameEnvironment, NameResolution, SymbolId,
    },
    source::SourceId,
};

use super::{
    AggregateProjectionDescriptor, CallDescriptor, ConstructionDescriptor,
    ContainerAppendDescriptor, ContainerClearDescriptor, ContainerConstructionDescriptor,
    ContainerInsertAtDescriptor, ContainerRemoveAtDescriptor, ContainerRemoveFirstDescriptor,
    ContainerRemoveLastDescriptor, ContainerSizeDescriptor, ElementPlaceDescriptor,
    ExpressionCategory, FunctionParameterType, IntrinsicCallable, OwnershipPrimitiveDescriptor,
    ParameterBindingDescriptor, ParameterMode, RcOperationDescriptor, StringOperationDescriptor,
    canonical::{CanonicalTypeId, CanonicalTypeKind, CanonicalTypeTable},
};

/// 由 classifier 声明 symbol 派生的稳定名义身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NominalId(SymbolId);

impl NominalId {
    pub(crate) const fn new(symbol: SymbolId) -> Self {
        Self(symbol)
    }

    /// 返回定义该名义类型的源码 symbol。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.0
    }
}

/// 名义声明的表示类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NominalKind {
    /// Inline value-semantics classifier.
    ValueClass,
    /// Heap reference-semantics classifier.
    Class,
    /// Static interface contract.
    Interface,
    /// Algebraic enum classifier.
    EnumClass,
    /// Named singleton classifier.
    Object,
}

/// typed 产物中的源码有序名义声明描述符。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NominalDescriptor {
    pub(crate) id: NominalId,
    pub(crate) kind: NominalKind,
    pub(crate) type_parameters: Vec<SymbolId>,
    pub(crate) direct_interfaces: Vec<TypeId>,
    pub(crate) interfaces: Vec<TypeId>,
    pub(crate) fields: Vec<SymbolId>,
    pub(crate) variants: Vec<SymbolId>,
    pub(crate) members: Vec<SymbolId>,
    pub(crate) deinit: Option<super::DeinitDescriptor>,
}

impl NominalDescriptor {
    /// 返回声明派生的名义身份。
    #[must_use]
    pub const fn id(&self) -> NominalId {
        self.id
    }
    /// 返回 classifier 表示类别。
    #[must_use]
    pub const fn kind(&self) -> NominalKind {
        self.kind
    }
    /// 返回源码声明顺序的类型参数 symbol。
    #[must_use]
    pub fn type_parameters(&self) -> &[SymbolId] {
        &self.type_parameters
    }
    /// 返回验证后的直接 interface 实例。
    #[must_use]
    pub fn direct_interfaces(&self) -> &[TypeId] {
        &self.direct_interfaces
    }
    /// 返回替换后的直接与传递 interface closure。
    #[must_use]
    pub fn interfaces(&self) -> &[TypeId] {
        &self.interfaces
    }
    /// 返回主构造器字段 symbol。
    #[must_use]
    pub fn fields(&self) -> &[SymbolId] {
        &self.fields
    }
    /// 返回 enum 变体 symbol。
    #[must_use]
    pub fn variants(&self) -> &[SymbolId] {
        &self.variants
    }
    /// 返回实例 member callable symbol。
    #[must_use]
    pub fn members(&self) -> &[SymbolId] {
        &self.members
    }
    /// 返回该名义类型是否声明了显式析构函数 `deinit`。
    #[must_use]
    pub const fn has_deinit(&self) -> bool {
        self.deinit.is_some()
    }

    /// 返回编译器隐式调用的析构 body 身份与只读 receiver 契约。
    #[must_use]
    pub const fn deinit(&self) -> Option<super::DeinitDescriptor> {
        self.deinit
    }
}

/// 类型参数的规范化单一上界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeParameterBound {
    /// Omitted or explicit top bound.
    Any,
    /// Static interface instance.
    Interface(TypeId),
    /// Compiler-owned structural capability.
    Capability(Capability),
    /// Invalid source bound retained for cascade suppression.
    Error,
}

/// typed 产物中的源码类型参数描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeParameterDescriptor {
    pub(crate) symbol: SymbolId,
    pub(crate) bound: TypeParameterBound,
}

/// 一个稳定 symbol 使用点已经由控制流证明为非空。
///
/// 描述符保留声明类型与窄化类型，使后续 lowering 无需重新解释条件表达式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonNullUseDescriptor {
    pub(crate) expression: ExpressionId,
    pub(crate) symbol: SymbolId,
    pub(crate) declared_type: TypeId,
    pub(crate) narrowed_type: TypeId,
}

/// 一个 null equality condition 对稳定 symbol 建立的 edge fact。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NullComparisonDescriptor {
    pub(crate) expression: ExpressionId,
    pub(crate) symbol: SymbolId,
    pub(crate) nullable_type: TypeId,
    pub(crate) non_null_when_true: bool,
}

impl NullComparisonDescriptor {
    /// 返回产生 edge fact 的 equality expression。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }
    /// 返回被比较的稳定 symbol。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.symbol
    }
    /// 返回 symbol 的 nullable 声明类型。
    #[must_use]
    pub const fn nullable_type(self) -> TypeId {
        self.nullable_type
    }
    /// 返回非空事实是否建立在 condition 的 true edge。
    #[must_use]
    pub const fn non_null_when_true(self) -> bool {
        self.non_null_when_true
    }
}

impl NonNullUseDescriptor {
    /// 返回被窄化的具体表达式使用点。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回该使用点解析到的稳定 symbol。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.symbol
    }

    /// 返回 symbol 的 nullable 声明类型。
    #[must_use]
    pub const fn declared_type(self) -> TypeId {
        self.declared_type
    }

    /// 返回控制流证明后的非空 inner 类型。
    #[must_use]
    pub const fn narrowed_type(self) -> TypeId {
        self.narrowed_type
    }
}

impl TypeParameterDescriptor {
    /// 返回声明 symbol。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.symbol
    }

    /// 返回规范化上界。
    #[must_use]
    pub const fn bound(self) -> TypeParameterBound {
        self.bound
    }
}

/// 经静态验证的接口委托转发计划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationPlan {
    pub(crate) owner: NominalId,
    pub(crate) interface: TypeId,
    pub(crate) target: SymbolId,
    pub(crate) delegation_span: crate::source::Span,
    pub(crate) by_span: crate::source::Span,
    pub(crate) forwarders: Vec<DelegationForwarderDescriptor>,
}

/// 一个 Borrow-only interface requirement 的单文件 delegate forwarder。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationForwarderDescriptor {
    pub(crate) requirement: SymbolId,
    pub(crate) receiver_type: TypeId,
    pub(crate) type_parameters: Vec<SymbolId>,
    pub(crate) parameters: Vec<FunctionParameterType>,
    pub(crate) return_type: TypeId,
    pub(crate) declaration_span: crate::source::Span,
}

impl DelegationForwarderDescriptor {
    /// 返回被转发的 interface member symbol。
    #[must_use]
    pub const fn requirement(&self) -> SymbolId {
        self.requirement
    }

    /// 返回完整 interface instance receiver 类型。
    #[must_use]
    pub const fn receiver_type(&self) -> TypeId {
        self.receiver_type
    }

    /// delegate forwarder receiver 固定为 Borrow。
    #[must_use]
    pub const fn receiver_mode(&self) -> ParameterMode {
        ParameterMode::Borrow
    }

    /// 返回 callable 类型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[SymbolId] {
        &self.type_parameters
    }

    /// 返回实例化后的显式参数契约。
    #[must_use]
    pub fn parameters(&self) -> &[FunctionParameterType] {
        &self.parameters
    }

    /// 返回实例化后的返回类型。
    #[must_use]
    pub const fn return_type(&self) -> TypeId {
        self.return_type
    }

    /// 返回 interface requirement 声明范围。
    #[must_use]
    pub const fn declaration_span(&self) -> crate::source::Span {
        self.declaration_span
    }
}

/// instance callable 的隐藏 receiver 契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallableReceiverDescriptor {
    pub(crate) mode: ParameterMode,
    pub(crate) ty: TypeId,
    pub(crate) declaration_span: crate::source::Span,
    pub(crate) marker_span: Option<crate::source::Span>,
}

impl CallableReceiverDescriptor {
    /// 返回规范化后的 Borrow/Inout/Value receiver mode。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回 owner 类型参数尚未实例化时的 receiver 类型模板。
    #[must_use]
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// 返回 callable 声明名范围。
    #[must_use]
    pub const fn declaration_span(self) -> crate::source::Span {
        self.declaration_span
    }

    /// 返回显式 receiver marker；缺省 Borrow 为 `None`。
    #[must_use]
    pub const fn marker_span(self) -> Option<crate::source::Span> {
        self.marker_span
    }
}

/// enum case 的 typed identity、root 实例模板与 payload 类型。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumCaseDescriptor {
    pub(crate) id: EnumCaseId,
    pub(crate) root: NominalId,
    pub(crate) root_type: TypeId,
    pub(crate) value_symbol: SymbolId,
    pub(crate) type_symbol: SymbolId,
    pub(crate) payloads: Vec<(SymbolId, TypeId)>,
}

impl EnumCaseDescriptor {
    /// 返回名称阶段分配的稳定 case 身份。
    #[must_use]
    pub const fn id(&self) -> EnumCaseId {
        self.id
    }
    /// 返回所属 root enum 的名义身份。
    #[must_use]
    pub const fn root(&self) -> NominalId {
        self.root
    }
    /// 返回携带 root 类型参数的声明内实例模板。
    #[must_use]
    pub const fn root_type(&self) -> TypeId {
        self.root_type
    }
    /// 返回值命名空间中的构造器/value symbol。
    #[must_use]
    pub const fn value_symbol(&self) -> SymbolId {
        self.value_symbol
    }
    /// 返回类型命名空间中的 case type symbol。
    #[must_use]
    pub const fn type_symbol(&self) -> SymbolId {
        self.type_symbol
    }
    /// 返回源码顺序的 payload symbol/type。
    #[must_use]
    pub fn payloads(&self) -> &[(SymbolId, TypeId)] {
        &self.payloads
    }
}

impl DelegationPlan {
    /// 返回拥有该委托的 ordinary class。
    #[must_use]
    pub const fn owner(&self) -> NominalId {
        self.owner
    }
    /// 返回完整 invariant interface 实例。
    #[must_use]
    pub const fn interface(&self) -> TypeId {
        self.interface
    }
    /// 返回同一主构造器的 immutable field symbol。
    #[must_use]
    pub const fn target(&self) -> SymbolId {
        self.target
    }

    /// 返回源码顺序的 Borrow-only delegate forwarder。
    #[must_use]
    pub fn forwarders(&self) -> &[DelegationForwarderDescriptor] {
        &self.forwarders
    }

    /// 返回完整 delegation clause 范围。
    #[must_use]
    pub const fn delegation_span(&self) -> crate::source::Span {
        self.delegation_span
    }

    /// 返回真实 `by` token 范围。
    #[must_use]
    pub const fn by_span(&self) -> crate::source::Span {
        self.by_span
    }

    pub(crate) fn sort_forwarders(&mut self) {
        self.forwarders.sort_by_key(|forwarder| {
            (
                forwarder.requirement.index(),
                forwarder.declaration_span.start(),
            )
        });
    }
}

use super::TypeCheckingError;

/// v0.22 基础阶段识别的内建类型身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuiltinType {
    /// 8-bit signed integer.
    Byte,
    /// 16-bit signed integer.
    Short,
    /// 32-bit signed integer.
    Int,
    /// 64-bit signed integer.
    Long,
    /// 8-bit unsigned integer.
    UByte,
    /// 16-bit unsigned integer.
    UShort,
    /// 32-bit unsigned integer.
    UInt,
    /// 64-bit unsigned integer.
    ULong,
    /// 32-bit IEEE-754 floating point.
    Float,
    /// 64-bit IEEE-754 floating point.
    Double,
    /// Boolean value.
    Boolean,
    /// Unicode scalar value.
    Char,
    /// String value.
    String,
    /// Unit value.
    Unit,
    /// Bottom type.
    Nothing,
    /// Top value type whose representation remains deferred.
    Any,
    /// Ordinary stored value with signed 32-bit integer endpoints.
    IntRange,
    /// Ordinary stored value with signed 64-bit integer endpoints.
    LongRange,
}

impl BuiltinType {
    /// 全部编译器内建类型的规范声明顺序。
    pub const ALL: [Self; 18] = [
        Self::Byte,
        Self::Short,
        Self::Int,
        Self::Long,
        Self::UByte,
        Self::UShort,
        Self::UInt,
        Self::ULong,
        Self::Float,
        Self::Double,
        Self::Boolean,
        Self::Char,
        Self::String,
        Self::Unit,
        Self::Nothing,
        Self::Any,
        Self::IntRange,
        Self::LongRange,
    ];

    /// 返回规范源码名称。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Byte => "Byte",
            Self::Short => "Short",
            Self::Int => "Int",
            Self::Long => "Long",
            Self::UByte => "UByte",
            Self::UShort => "UShort",
            Self::UInt => "UInt",
            Self::ULong => "ULong",
            Self::Float => "Float",
            Self::Double => "Double",
            Self::Boolean => "Boolean",
            Self::Char => "Char",
            Self::String => "String",
            Self::Unit => "Unit",
            Self::Nothing => "Nothing",
            Self::Any => "Any",
            Self::IntRange => "IntRange",
            Self::LongRange => "LongRange",
        }
    }
}

/// 编译器预声明、由后续阶段结构化求值的封闭能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Value can be duplicated without user-defined copy glue.
    Copyable,
    /// Value can be transferred across threads.
    Transferable,
    /// Value can be hashed and compared for map key equivalence.
    Hashable,
}

/// 由类型环境显式绑定、不能由源码同名声明冒充的内建类型构造器。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntrinsicTypeConstructor {
    /// Exclusive heap owner for one concrete value-class instance.
    Box,
    /// Reference-counted shared owner; runtime API remains a Phase 5 concern.
    Rc,
    /// Fixed-length mutable-element sequential owner.
    Array,
    /// Read-only sequential owner.
    List,
    /// Growable sequential owner.
    MutableList,
    /// Read-only associative key-value owner.
    Map,
    /// Growable associative key-value owner.
    MutableMap,
    /// Single-source, non-escaping inline range descriptor; owns no elements.
    View,
}

impl IntrinsicTypeConstructor {
    /// 期望的泛型类型实参数量。
    #[must_use]
    pub const fn expected_type_argument_count(self) -> usize {
        match self {
            Self::Box | Self::Rc | Self::Array | Self::List | Self::MutableList | Self::View => 1,
            Self::Map | Self::MutableMap => 2,
        }
    }
}

/// 一个规范化类型在当前静态上下文中的复制能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Copyability {
    /// Values can be duplicated without observable copy or drop glue.
    Copyable,
    /// No copy proof exists, so by-value uses must move.
    MoveOnly,
    /// A later typed selection still determines the type.
    Unknown,
    /// The source type or its inline layout is invalid.
    Error,
}

/// 局部结构化解构交给 Phase 3 的原子所有权模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestructuringMode {
    /// Each component is copied and the source remains available.
    Copy,
    /// The whole source is consumed as one ownership action.
    Consume,
}

/// 一个已类型化的结构化解构分量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructuringComponent {
    symbol: SymbolId,
    ty: TypeId,
}

impl DestructuringComponent {
    pub(crate) const fn new(symbol: SymbolId, ty: TypeId) -> Self {
        Self { symbol, ty }
    }

    /// 返回接收该分量的局部 binding symbol。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.symbol
    }

    /// 返回替换实际泛型实参后的分量类型。
    #[must_use]
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// 一条精确、有效的局部 value-class 结构化解构。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DestructuringDescriptor {
    statement: StatementId,
    source_type: TypeId,
    mode: DestructuringMode,
    components: Vec<DestructuringComponent>,
}

impl DestructuringDescriptor {
    pub(crate) fn new(
        statement: StatementId,
        source_type: TypeId,
        mode: DestructuringMode,
        components: Vec<DestructuringComponent>,
    ) -> Self {
        Self {
            statement,
            source_type,
            mode,
            components,
        }
    }

    /// 返回拥有该操作的稳定 statement identity。
    #[must_use]
    pub const fn statement(&self) -> StatementId {
        self.statement
    }

    /// 返回 initializer 的规范化源类型。
    #[must_use]
    pub const fn source_type(&self) -> TypeId {
        self.source_type
    }

    /// 返回 Copy 或 Consume 原子模式。
    #[must_use]
    pub const fn mode(&self) -> DestructuringMode {
        self.mode
    }

    /// 返回字段声明顺序的 binding / component type。
    #[must_use]
    pub fn components(&self) -> &[DestructuringComponent] {
        &self.components
    }
}

/// 外部签名使用的递归类型描述；进入 typed 产物后会被规范化为 [`TypeId`]。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentType {
    /// Builtin scalar or special type.
    Builtin(BuiltinType),
    /// Nullable wrapper.
    Nullable(Box<EnvironmentType>),
    /// Function type.
    Function {
        /// Whether the callable owns all captures.
        move_only: bool,
        /// Ordered parameter types.
        parameters: Vec<EnvironmentParameter>,
        /// Return type.
        return_type: Box<EnvironmentType>,
    },
}

/// 外部函数类型中的一个参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentParameter {
    /// Parameter passing mode.
    pub mode: ParameterMode,
    /// Parameter value type.
    pub ty: EnvironmentType,
}

/// 外部具名函数的单态签名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentFunction {
    /// Ordered parameters.
    pub parameters: Vec<EnvironmentParameter>,
    /// Declared return type.
    pub return_type: EnvironmentType,
    /// Compiler-bound effects; source callables cannot acquire these by spelling.
    pub effects: Vec<EnvironmentFunctionEffect>,
}

/// An effect attached to a predeclared callable identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentFunctionEffect {
    /// The selected parameter crosses a thread boundary.
    CrossThreadTransfer {
        /// Zero-based parameter index in the same environment signature.
        parameter: usize,
    },
    /// The call terminates the process through the compiler-owned abort primitive.
    Abort,
    /// The call writes one UTF-8 line through the compiler-owned stdout primitive.
    PrintLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalTypeBinding {
    Builtin(BuiltinType),
    Capability(Capability),
    Intrinsic(IntrinsicTypeConstructor),
    Value(EnvironmentType),
    Function(EnvironmentFunction),
    IntrinsicCallable(IntrinsicCallable),
}

/// 与一个显式 [`NameEnvironment`] 绑定的不可变类型环境构建器。
#[derive(Clone, Debug)]
pub struct TypeEnvironment {
    owner: Arc<()>,
    symbol_kinds: Vec<ExternalSymbolKind>,
    bindings: BTreeMap<ExternalSymbolId, ExternalTypeBinding>,
    pub(super) range_sources: Vec<SourceId>,
    pub(super) range_extension_sources: Vec<SourceId>,
    pub(super) text_sources: Vec<SourceId>,
}

impl TypeEnvironment {
    /// 创建与给定名称环境共享身份的空类型环境。
    #[must_use]
    pub fn new(names: &NameEnvironment) -> Self {
        Self {
            owner: names.owner(),
            symbol_kinds: names.symbols().iter().map(|symbol| symbol.kind()).collect(),
            bindings: BTreeMap::new(),
            range_sources: Vec::new(),
            range_extension_sources: Vec::new(),
            text_sources: Vec::new(),
        }
    }

    /// 把外部 type symbol 绑定为内建类型身份。
    pub fn bind_builtin(
        &mut self,
        symbol: ExternalSymbolId,
        builtin: BuiltinType,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::Builtin(builtin),
            ExternalSymbolKind::Type,
        )
    }

    /// 把外部 type symbol 绑定为编译器封闭能力身份。
    pub fn bind_capability(
        &mut self,
        symbol: ExternalSymbolId,
        capability: Capability,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::Capability(capability),
            ExternalSymbolKind::Type,
        )
    }

    /// 把外部 type symbol 绑定为编译器拥有的内建类型构造器身份。
    pub fn bind_intrinsic(
        &mut self,
        symbol: ExternalSymbolId,
        intrinsic: IntrinsicTypeConstructor,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::Intrinsic(intrinsic),
            ExternalSymbolKind::Type,
        )
    }

    /// 为外部 value symbol 提供单态类型。
    pub fn bind_value(
        &mut self,
        symbol: ExternalSymbolId,
        ty: EnvironmentType,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::Value(ty),
            ExternalSymbolKind::Value,
        )
    }

    /// 为外部 function symbol 提供一个单态签名。
    pub fn bind_function(
        &mut self,
        symbol: ExternalSymbolId,
        signature: EnvironmentFunction,
    ) -> Result<(), TypeCheckingError> {
        if signature.effects.iter().any(|effect| match effect {
            EnvironmentFunctionEffect::CrossThreadTransfer { parameter } => signature
                .parameters
                .get(*parameter)
                .is_none_or(|parameter| parameter.mode != ParameterMode::Value),
            EnvironmentFunctionEffect::Abort => {
                signature.parameters.as_slice()
                    != [EnvironmentParameter {
                        mode: ParameterMode::Borrow,
                        ty: EnvironmentType::Builtin(BuiltinType::String),
                    }]
                    || signature.return_type != EnvironmentType::Builtin(BuiltinType::Nothing)
            }
            EnvironmentFunctionEffect::PrintLine => {
                signature.parameters.as_slice()
                    != [EnvironmentParameter {
                        mode: ParameterMode::Borrow,
                        ty: EnvironmentType::Builtin(BuiltinType::String),
                    }]
                    || signature.return_type != EnvironmentType::Builtin(BuiltinType::Unit)
            }
        }) {
            return Err(TypeCheckingError::InvalidExternalBinding);
        }
        self.bind(
            symbol,
            ExternalTypeBinding::Function(signature),
            ExternalSymbolKind::Function,
        )
    }

    /// 把外部 function symbol 绑定为编译器拥有的封闭核心构造。
    pub fn bind_intrinsic_callable(
        &mut self,
        symbol: ExternalSymbolId,
        callable: IntrinsicCallable,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::IntrinsicCallable(callable),
            ExternalSymbolKind::Function,
        )
    }

    fn bind(
        &mut self,
        symbol: ExternalSymbolId,
        binding: ExternalTypeBinding,
        expected: ExternalSymbolKind,
    ) -> Result<(), TypeCheckingError> {
        if self.symbol_kinds.get(symbol.index()).copied() != Some(expected)
            || self.bindings.contains_key(&symbol)
        {
            return Err(TypeCheckingError::InvalidExternalBinding);
        }
        self.bindings.insert(symbol, binding);
        Ok(())
    }

    pub(crate) fn owner(&self) -> &Arc<()> {
        &self.owner
    }

    pub(crate) fn binding(&self, id: ExternalSymbolId) -> Option<&ExternalTypeBinding> {
        self.bindings.get(&id)
    }
}

/// Phase 2 的单文件 typed 产物。
#[derive(Clone, Debug)]
pub struct TypedFile {
    pub(super) resource_classifications:
        super::resource::ResourceCache<super::resource::FileResourceClassifier>,
    constants: Option<super::ValidatedConstants>,
    source_id: SourceId,
    environment_owner: Arc<()>,
    name_analysis_owner: Arc<()>,
    analysis_owner: Arc<()>,
    types: TypeTable,
    expression_types: Vec<TypeId>,
    type_ref_types: Vec<TypeId>,
    symbol_types: Vec<TypeId>,
    parameter_bindings: Vec<ParameterBindingDescriptor>,
    non_null_uses: Vec<NonNullUseDescriptor>,
    null_comparisons: Vec<NullComparisonDescriptor>,
    nullable_whens: Vec<super::NullableWhenDescriptor>,
    non_null_assertions: Vec<super::NonNullAssertionDescriptor>,
    nominals: Vec<NominalDescriptor>,
    type_parameters: Vec<TypeParameterDescriptor>,
    delegations: Vec<DelegationPlan>,
    callables: Vec<CallableDescriptor>,
    enum_cases: Vec<EnumCaseDescriptor>,
    copyabilities: Vec<Copyability>,
    destructurings: Vec<DestructuringDescriptor>,
    pub(super) iterations: super::iteration::FileIterations,
    expression_categories: Vec<ExpressionCategory>,
    calls: Vec<CallDescriptor>,
    constructions: Vec<ConstructionDescriptor>,
    aggregate_projections: Vec<AggregateProjectionDescriptor>,
    ownership_primitives: Vec<OwnershipPrimitiveDescriptor>,
    rc_operations: Vec<RcOperationDescriptor>,
    string_operations: Vec<StringOperationDescriptor>,
    integer_operations: Vec<IntegerOperationDescriptor>,
    integer_range_operations: Vec<IntegerRangeOperationDescriptor>,
    pub(crate) container_constructions: Vec<ContainerConstructionDescriptor>,
    pub(crate) container_sizes: Vec<ContainerSizeDescriptor>,
    pub(crate) range_sizes: Vec<super::RangeSizeDescriptor<ExpressionId, TypeId>>,
    pub(crate) container_appends: Vec<ContainerAppendDescriptor>,
    pub(crate) container_clears: Vec<ContainerClearDescriptor>,
    pub(crate) container_remove_ats: Vec<ContainerRemoveAtDescriptor>,
    pub(crate) container_remove_lasts: Vec<ContainerRemoveLastDescriptor>,
    pub(crate) container_remove_firsts: Vec<ContainerRemoveFirstDescriptor>,
    pub(crate) container_insert_ats: Vec<ContainerInsertAtDescriptor>,
    pub(crate) element_places: Vec<ElementPlaceDescriptor>,
    pub(crate) map_descriptors: super::map_descriptor::MapDescriptors,
    diagnostics: Vec<Diagnostic>,
}

pub(crate) struct TypedFileParts {
    pub(crate) constants: Option<super::ValidatedConstants>,
    pub(crate) expression_types: Vec<TypeId>,
    pub(crate) type_ref_types: Vec<TypeId>,
    pub(crate) symbol_types: Vec<TypeId>,
    pub(crate) parameter_bindings: Vec<ParameterBindingDescriptor>,
    pub(crate) non_null_uses: Vec<NonNullUseDescriptor>,
    pub(crate) null_comparisons: Vec<NullComparisonDescriptor>,
    pub(crate) nullable_whens: Vec<super::NullableWhenDescriptor>,
    pub(crate) non_null_assertions: Vec<super::NonNullAssertionDescriptor>,
    pub(crate) nominals: Vec<NominalDescriptor>,
    pub(crate) type_parameters: Vec<TypeParameterDescriptor>,
    pub(crate) delegations: Vec<DelegationPlan>,
    pub(crate) callables: Vec<CallableDescriptor>,
    pub(crate) enum_cases: Vec<EnumCaseDescriptor>,
    pub(crate) copyabilities: Vec<Copyability>,
    pub(crate) destructurings: Vec<DestructuringDescriptor>,
    pub(crate) iterations: super::iteration::FileIterations,
    pub(crate) expression_categories: Vec<ExpressionCategory>,
    pub(crate) calls: Vec<CallDescriptor>,
    pub(crate) constructions: Vec<ConstructionDescriptor>,
    pub(crate) aggregate_projections: Vec<AggregateProjectionDescriptor>,
    pub(crate) ownership_primitives: Vec<OwnershipPrimitiveDescriptor>,
    pub(crate) rc_operations: Vec<RcOperationDescriptor>,
    pub(crate) string_operations: Vec<StringOperationDescriptor>,
    pub(crate) integer_operations: Vec<IntegerOperationDescriptor>,
    pub(crate) integer_range_operations: Vec<IntegerRangeOperationDescriptor>,
    pub(crate) container_constructions: Vec<ContainerConstructionDescriptor>,
    pub(crate) container_sizes: Vec<ContainerSizeDescriptor>,
    pub(crate) range_sizes: Vec<super::RangeSizeDescriptor<ExpressionId, TypeId>>,
    pub(crate) container_appends: Vec<ContainerAppendDescriptor>,
    pub(crate) container_clears: Vec<ContainerClearDescriptor>,
    pub(crate) container_remove_ats: Vec<ContainerRemoveAtDescriptor>,
    pub(crate) container_remove_lasts: Vec<ContainerRemoveLastDescriptor>,
    pub(crate) container_remove_firsts: Vec<ContainerRemoveFirstDescriptor>,
    pub(crate) container_insert_ats: Vec<ContainerInsertAtDescriptor>,
    pub(crate) element_places: Vec<ElementPlaceDescriptor>,
    pub(crate) map_descriptors: super::map_descriptor::MapDescriptors,
}
