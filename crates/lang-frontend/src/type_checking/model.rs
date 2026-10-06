use crate::type_checking::IntegerOperationDescriptor;
use std::{collections::BTreeMap, sync::Arc};

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
    ContainerRemoveAtDescriptor, ContainerRemoveLastDescriptor, ContainerSizeDescriptor,
    ElementPlaceDescriptor, ExpressionCategory, FunctionParameterType, IntrinsicCallable,
    OwnershipPrimitiveDescriptor, ParameterBindingDescriptor, ParameterMode, RcOperationDescriptor,
    StringOperationDescriptor,
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

/// 已规范化的顶层或实例 member callable 签名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableDescriptor {
    pub(crate) symbol: SymbolId,
    pub(crate) owner: Option<NominalId>,
    pub(crate) receiver: Option<CallableReceiverDescriptor>,
    pub(crate) type_parameters: Vec<SymbolId>,
    pub(crate) parameter_symbols: Vec<Option<SymbolId>>,
    pub(crate) parameters: Vec<FunctionParameterType>,
    pub(crate) return_type: TypeId,
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

impl CallableDescriptor {
    /// 返回函数声明 symbol。
    #[must_use]
    pub const fn symbol(&self) -> SymbolId {
        self.symbol
    }
    /// 返回实例 member owner；顶层函数为 `None`。
    #[must_use]
    pub const fn owner(&self) -> Option<NominalId> {
        self.owner
    }
    /// 返回 instance member 的隐藏 receiver 契约；顶层 callable 为 `None`。
    #[must_use]
    pub const fn receiver(&self) -> Option<CallableReceiverDescriptor> {
        self.receiver
    }
    /// 返回 callable 自身的源码顺序类型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[SymbolId] {
        &self.type_parameters
    }
    /// 返回与参数顺序对齐的稳定名称 symbol；恢复参数为 `None`。
    #[must_use]
    pub fn parameter_symbols(&self) -> &[Option<SymbolId>] {
        &self.parameter_symbols
    }
    /// 返回包含参数模式的规范化参数。
    #[must_use]
    pub fn parameters(&self) -> &[FunctionParameterType] {
        &self.parameters
    }
    /// 返回规范化返回类型。
    #[must_use]
    pub const fn return_type(&self) -> TypeId {
        self.return_type
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
}

impl BuiltinType {
    /// 全部编译器内建类型的规范声明顺序。
    pub const ALL: [Self; 16] = [
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
}

impl TypeEnvironment {
    /// 创建与给定名称环境共享身份的空类型环境。
    #[must_use]
    pub fn new(names: &NameEnvironment) -> Self {
        Self {
            owner: names.owner(),
            symbol_kinds: names.symbols().iter().map(|symbol| symbol.kind()).collect(),
            bindings: BTreeMap::new(),
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

/// 一次 typed 产物内稳定的规范化类型身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(usize);

impl TypeId {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    /// 返回 typed 产物类型表中的稳定下标。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

impl CanonicalTypeId for TypeId {
    fn from_index(index: usize) -> Self {
        Self::new(index)
    }

    fn index(self) -> usize {
        self.index()
    }
}

/// 后续阶段必须精确承接的 deferred 原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeferredReason {
    /// `Any` representation awaits the later nominal model.
    AnyValueRepresentation,
    /// Nominal or type-parameter identity is not in this Spec.
    NominalOrTypeParameter,
    /// Qualified type lookup awaits member/type resolution.
    QualifiedType,
    /// Function structure contains a deferred component.
    FunctionContainsDeferred,
    /// External symbol has no type binding.
    UnboundExternalType,
    /// Value type depends on a later declaration.
    ForwardValueType,
    /// An overload set needs candidate selection.
    OverloadSelection,
    /// `this` needs classifier typing.
    ThisType,
    /// Member lookup is deferred.
    MemberAccess,
    /// Call resolution is deferred.
    Call,
    /// Index contract resolution is deferred.
    Index,
    /// Assignment/place checking is deferred.
    Assignment,
    /// Cast or type-test semantics are deferred.
    CastOrTypeTest,
    /// Result propagation is deferred.
    ErrorPropagation,
    /// `when` typing and exhaustiveness are deferred.
    WhenTyping,
    /// A control-flow join depends on later typing.
    ControlJoin,
    /// Destructuring component typing is deferred.
    Destructuring,
    /// Iteration source typing is deferred.
    LoopSource,
}

/// 整数字面量在默认或 expected-type 定型前的约束族。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegerConstraint {
    /// Unsuffixed signed literal family.
    Signed,
    /// Unsuffixed unsigned literal family.
    Unsigned,
}

/// 类型表中的规范化结构。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeKind {
    /// Builtin scalar or special type.
    Builtin(BuiltinType),
    /// Nullable wrapper.
    Nullable(TypeId),
    /// Function type.
    Function {
        /// Whether the callable owns all captures.
        move_only: bool,
        /// Ordered parameters.
        parameters: Vec<FunctionParameterType>,
        /// Return type.
        return_type: TypeId,
    },
    /// Invariant instantiation of a source nominal declaration.
    Nominal {
        /// Source classifier identity.
        nominal: NominalId,
        /// Ordered invariant type arguments.
        arguments: Vec<TypeId>,
    },
    /// Compiler-owned intrinsic type constructor applied to invariant arguments.
    Intrinsic {
        /// Stable constructor identity supplied by [`TypeEnvironment`].
        constructor: IntrinsicTypeConstructor,
        /// Ordered invariant arguments.
        arguments: Vec<TypeId>,
    },
    /// 仅在 type-test 与流事实中存在的 enum case refinement。
    EnumCase {
        /// 名称阶段稳定 case identity。
        case: EnumCaseId,
        /// 携带实际泛型参数的 root enum runtime type。
        root: TypeId,
    },
    /// Source type parameter identity.
    TypeParameter(SymbolId),
    /// Static `Self` inside an interface default body.
    StaticSelf(TypeId),
    /// Predeclared structural capability used only as a generic bound.
    Capability(Capability),
    /// Integer literal constraint before contextual selection.
    IntegerLiteral(IntegerConstraint),
    /// Error recovery type.
    Error,
    /// Precisely classified work for a later Spec.
    Deferred(DeferredReason),
}

impl CanonicalTypeKind for TypeKind {
    type Id = TypeId;

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

/// 插入顺序稳定并按结构去重的类型表。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTable {
    canonical: CanonicalTypeTable<TypeKind>,
}

impl TypeTable {
    pub(crate) fn new() -> Self {
        Self {
            canonical: CanonicalTypeTable::new(),
        }
    }

    pub(crate) fn intern(&mut self, kind: TypeKind) -> TypeId {
        self.canonical.intern(kind)
    }

    /// 按身份读取规范化结构。
    #[must_use]
    pub fn get(&self, id: TypeId) -> Option<&TypeKind> {
        self.canonical.get(id)
    }

    /// 查询本次 typed 产物中已规范化的类型，不创建或修改类型身份。
    #[must_use]
    pub fn find(&self, kind: &TypeKind) -> Option<TypeId> {
        self.canonical.find(kind)
    }

    /// 查询本次 typed 产物中的内建类型身份。
    #[must_use]
    pub fn builtin(&self, builtin: BuiltinType) -> Option<TypeId> {
        self.canonical.builtin(builtin)
    }

    /// 返回类型表大小。
    #[must_use]
    pub fn len(&self) -> usize {
        self.canonical.len()
    }

    /// 返回类型表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty()
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
    iterations: Vec<crate::type_checking::SequentialIterationDescriptor>,
    expression_categories: Vec<ExpressionCategory>,
    calls: Vec<CallDescriptor>,
    constructions: Vec<ConstructionDescriptor>,
    aggregate_projections: Vec<AggregateProjectionDescriptor>,
    ownership_primitives: Vec<OwnershipPrimitiveDescriptor>,
    rc_operations: Vec<RcOperationDescriptor>,
    string_operations: Vec<StringOperationDescriptor>,
    integer_operations: Vec<IntegerOperationDescriptor>,
    pub(crate) container_constructions: Vec<ContainerConstructionDescriptor>,
    pub(crate) container_sizes: Vec<ContainerSizeDescriptor>,
    pub(crate) container_appends: Vec<ContainerAppendDescriptor>,
    pub(crate) container_clears: Vec<ContainerClearDescriptor>,
    pub(crate) container_remove_ats: Vec<ContainerRemoveAtDescriptor>,
    pub(crate) container_remove_lasts: Vec<ContainerRemoveLastDescriptor>,
    pub(crate) element_places: Vec<ElementPlaceDescriptor>,
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
    pub(crate) iterations: Vec<crate::type_checking::SequentialIterationDescriptor>,
    pub(crate) expression_categories: Vec<ExpressionCategory>,
    pub(crate) calls: Vec<CallDescriptor>,
    pub(crate) constructions: Vec<ConstructionDescriptor>,
    pub(crate) aggregate_projections: Vec<AggregateProjectionDescriptor>,
    pub(crate) ownership_primitives: Vec<OwnershipPrimitiveDescriptor>,
    pub(crate) rc_operations: Vec<RcOperationDescriptor>,
    pub(crate) string_operations: Vec<StringOperationDescriptor>,
    pub(crate) integer_operations: Vec<IntegerOperationDescriptor>,
    pub(crate) container_constructions: Vec<ContainerConstructionDescriptor>,
    pub(crate) container_sizes: Vec<ContainerSizeDescriptor>,
    pub(crate) container_appends: Vec<ContainerAppendDescriptor>,
    pub(crate) container_clears: Vec<ContainerClearDescriptor>,
    pub(crate) container_remove_ats: Vec<ContainerRemoveAtDescriptor>,
    pub(crate) container_remove_lasts: Vec<ContainerRemoveLastDescriptor>,
    pub(crate) element_places: Vec<ElementPlaceDescriptor>,
}

impl TypedFile {
    pub(crate) fn new(
        source_id: SourceId,
        environment_owner: Arc<()>,
        name_analysis_owner: Arc<()>,
        types: TypeTable,
        parts: TypedFileParts,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            resource_classifications: super::resource::ResourceCache::new(),
            source_id,
            environment_owner,
            name_analysis_owner,
            analysis_owner: parts
                .constants
                .as_ref()
                .map_or_else(|| Arc::new(()), |facts| facts.analysis_owner.clone()),
            constants: parts.constants,
            types,
            expression_types: parts.expression_types,
            type_ref_types: parts.type_ref_types,
            symbol_types: parts.symbol_types,
            parameter_bindings: parts.parameter_bindings,
            non_null_uses: parts.non_null_uses,
            null_comparisons: parts.null_comparisons,
            nullable_whens: parts.nullable_whens,
            non_null_assertions: parts.non_null_assertions,
            nominals: parts.nominals,
            type_parameters: parts.type_parameters,
            delegations: parts.delegations,
            callables: parts.callables,
            enum_cases: parts.enum_cases,
            copyabilities: parts.copyabilities,
            destructurings: parts.destructurings,
            iterations: parts.iterations,
            expression_categories: parts.expression_categories,
            calls: parts.calls,
            constructions: parts.constructions,
            aggregate_projections: parts.aggregate_projections,
            ownership_primitives: parts.ownership_primitives,
            rc_operations: parts.rc_operations,
            string_operations: parts.string_operations,
            integer_operations: parts.integer_operations,
            container_constructions: parts.container_constructions,
            container_sizes: parts.container_sizes,
            container_appends: parts.container_appends,
            container_clears: parts.container_clears,
            container_remove_ats: parts.container_remove_ats,
            container_remove_lasts: parts.container_remove_lasts,
            element_places: parts.element_places,
            diagnostics,
        }
    }

    #[must_use]
    /// 返回来源文件身份。
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// 返回该 typed 产物是否与名称产物共享源码和显式环境身份。
    #[must_use]
    pub fn is_compatible_with_names(&self, names: &NameResolution) -> bool {
        self.source_id == names.source_id()
            && Arc::ptr_eq(&self.environment_owner, names.environment_owner())
            && Arc::ptr_eq(&self.name_analysis_owner, names.analysis_owner())
    }

    pub(crate) fn environment_owner(&self) -> &Arc<()> {
        &self.environment_owner
    }

    pub(crate) fn analysis_owner(&self) -> &Arc<()> {
        &self.analysis_owner
    }

    #[must_use]
    /// 返回本产物的规范化类型表。
    pub const fn types(&self) -> &TypeTable {
        &self.types
    }

    #[must_use]
    /// 查询表达式类型。
    pub fn expression_type(&self, id: ExpressionId) -> Option<TypeId> {
        self.expression_types.get(id.index()).copied()
    }

    #[must_use]
    /// 查询 TypeRef 类型。
    pub fn type_ref_type(&self, id: TypeRefId) -> Option<TypeId> {
        self.type_ref_types.get(id.index()).copied()
    }

    #[must_use]
    /// 查询源码 symbol 类型。
    pub fn symbol_type(&self, id: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(id.index()).copied()
    }

    /// 查询一个具名函数或已采用 expected contract 的 lambda 参数模式。
    #[must_use]
    pub fn parameter_mode(&self, id: SymbolId) -> Option<ParameterMode> {
        self.parameter_bindings
            .binary_search_by_key(&id.index(), |binding| binding.symbol().index())
            .ok()
            .map(|index| self.parameter_bindings[index].mode())
    }

    /// 返回稳定 symbol 顺序的 callable 参数绑定事实。
    #[must_use]
    pub fn parameter_bindings(&self) -> &[ParameterBindingDescriptor] {
        &self.parameter_bindings
    }

    /// 返回按 expression identity 排序的已证明非空使用点。
    #[must_use]
    pub fn non_null_uses(&self) -> &[NonNullUseDescriptor] {
        &self.non_null_uses
    }

    /// 查询一个表达式是否是已证明非空的稳定 symbol 使用点。
    #[must_use]
    pub fn non_null_use(&self, expression: ExpressionId) -> Option<NonNullUseDescriptor> {
        self.non_null_uses
            .binary_search_by_key(&expression.index(), |descriptor| {
                descriptor.expression.index()
            })
            .ok()
            .map(|index| self.non_null_uses[index])
    }

    /// 查询一个 equality expression 发布的 null/non-null edge fact。
    #[must_use]
    pub fn null_comparison(&self, expression: ExpressionId) -> Option<NullComparisonDescriptor> {
        self.null_comparisons
            .binary_search_by_key(&expression.index(), |descriptor| {
                descriptor.expression.index()
            })
            .ok()
            .map(|index| self.null_comparisons[index])
    }

    /// 返回源码声明顺序的名义类型描述符。
    #[must_use]
    pub fn nominals(&self) -> &[NominalDescriptor] {
        &self.nominals
    }

    /// 返回源码 symbol 顺序的类型参数描述符。
    #[must_use]
    pub fn type_parameters(&self) -> &[TypeParameterDescriptor] {
        &self.type_parameters
    }

    /// 返回源码顺序的有效静态接口委托计划。
    #[must_use]
    pub fn delegations(&self) -> &[DelegationPlan] {
        &self.delegations
    }

    /// 返回源码声明顺序的已知 callable 签名。
    #[must_use]
    pub fn callables(&self) -> &[CallableDescriptor] {
        &self.callables
    }

    /// 返回源码声明顺序的 typed enum case 描述符。
    #[must_use]
    pub fn enum_cases(&self) -> &[EnumCaseDescriptor] {
        &self.enum_cases
    }

    /// 查询一个本产物 TypeId 的静态复制能力。
    #[must_use]
    pub fn copyability(&self, id: TypeId) -> Option<Copyability> {
        self.copyabilities.get(id.index()).copied()
    }

    /// 已完成类型检查的 for 计划，按 StatementId 排序；不授予所有权能力。
    #[must_use]
    pub fn sequential_iterations(&self) -> &[crate::type_checking::SequentialIterationDescriptor] {
        &self.iterations
    }
    /// 查询指定 for 的完整计划；非法或 poisoned for 不发布。
    #[must_use]
    pub fn sequential_iteration(
        &self,
        statement: StatementId,
    ) -> Option<&crate::type_checking::SequentialIterationDescriptor> {
        self.iterations
            .iter()
            .find(|plan| plan.statement() == statement)
    }

    /// 返回源码 statement 顺序的有效 value-class 解构描述符。
    #[must_use]
    pub fn destructurings(&self) -> &[DestructuringDescriptor] {
        &self.destructurings
    }

    /// 查询指定 statement 的有效 value-class 解构描述符。
    #[must_use]
    pub fn destructuring(&self, statement: StatementId) -> Option<&DestructuringDescriptor> {
        self.destructurings
            .iter()
            .find(|descriptor| descriptor.statement() == statement)
    }

    /// 查询一个表达式的 Phase 2 place/temporary 类别。
    #[must_use]
    pub fn expression_category(&self, expression: ExpressionId) -> Option<ExpressionCategory> {
        self.expression_categories.get(expression.index()).copied()
    }

    /// 返回源码 call expression 顺序的成功选择描述符。
    #[must_use]
    pub fn calls(&self) -> &[CallDescriptor] {
        &self.calls
    }

    /// 查询指定 call expression 的成功选择描述符。
    #[must_use]
    pub fn call(&self, expression: ExpressionId) -> Option<&CallDescriptor> {
        self.calls
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码 expression 顺序的成功 construction descriptors。
    #[must_use]
    pub fn constructions(&self) -> &[ConstructionDescriptor] {
        &self.constructions
    }

    /// 查询指定 expression 的成功 construction descriptor。
    #[must_use]
    pub fn construction(&self, expression: ExpressionId) -> Option<&ConstructionDescriptor> {
        self.constructions
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码 expression 顺序的聚合分量投影。
    #[must_use]
    pub fn aggregate_projections(&self) -> &[AggregateProjectionDescriptor] {
        &self.aggregate_projections
    }

    /// 查询指定 expression 的聚合分量投影。
    #[must_use]
    pub fn aggregate_projection(
        &self,
        expression: ExpressionId,
    ) -> Option<AggregateProjectionDescriptor> {
        self.aggregate_projections
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 String intrinsic 操作。
    #[must_use]
    pub fn string_operations(&self) -> &[StringOperationDescriptor] {
        &self.string_operations
    }

    /// 查询已绑定 receiver 的显式 String 操作。
    #[must_use]
    pub fn string_operation(&self, expression: ExpressionId) -> Option<StringOperationDescriptor> {
        self.string_operations
            .iter()
            .copied()
            .find(|operation| operation.expression() == expression)
    }

    /// 返回源码稳定顺序的 Integer intrinsic 操作。
    #[must_use]
    pub fn integer_operations(&self) -> &[IntegerOperationDescriptor] {
        &self.integer_operations
    }

    /// 查询已绑定 receiver 的显式 Integer 操作。
    #[must_use]
    pub fn integer_operation(
        &self,
        expression: ExpressionId,
    ) -> Option<IntegerOperationDescriptor> {
        self.integer_operations
            .iter()
            .copied()
            .find(|operation| operation.expression() == expression)
    }

    /// 返回源码稳定顺序的原子所有权原语静态事实；错误产物不发布此表。
    #[must_use]
    pub fn ownership_primitives(&self) -> &[OwnershipPrimitiveDescriptor] {
        &self.ownership_primitives
    }

    /// 查询 compiler-bound 原语；普通同名源码调用返回 None。
    #[must_use]
    pub fn ownership_primitive(
        &self,
        expression: ExpressionId,
    ) -> Option<OwnershipPrimitiveDescriptor> {
        self.ownership_primitives
            .iter()
            .copied()
            .find(|fact| fact.expression() == expression)
    }

    /// 返回源码 expression 顺序的 intrinsic `Rc<T>` 操作。
    #[must_use]
    pub fn rc_operations(&self) -> &[RcOperationDescriptor] {
        &self.rc_operations
    }

    /// 查询指定 expression 的 intrinsic `Rc<T>` 操作。
    #[must_use]
    pub fn rc_operation(&self, expression: ExpressionId) -> Option<RcOperationDescriptor> {
        self.rc_operations
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    #[must_use]
    /// 返回稳定排序的类型诊断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl TypedFile {
    /// 完整常量事实；上游或类型诊断、未完成求值时不发布半成品。
    #[must_use]
    pub fn constants(&self) -> Option<&super::ValidatedConstants> {
        self.constants.as_ref()
    }

    /// 返回按 when expression identity 排序的 nullable flow plans。
    #[must_use]
    pub fn nullable_whens(&self) -> &[super::NullableWhenDescriptor] {
        &self.nullable_whens
    }
    /// 查询单次 subject 求值对应的 nullable flow plan。
    #[must_use]
    pub fn nullable_when(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::NullableWhenDescriptor> {
        self.nullable_whens
            .iter()
            .find(|plan| plan.expression() == expression)
    }
}

impl TypedFile {
    /// 按 assertion AST identity 排序的单次求值 extraction 候选。
    #[must_use]
    pub fn non_null_assertions(&self) -> &[super::NonNullAssertionDescriptor] {
        &self.non_null_assertions
    }

    /// 查询 compiler-bound assertion，不经过普通 callable 名称选择。
    #[must_use]
    pub fn non_null_assertion(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::NonNullAssertionDescriptor> {
        self.non_null_assertions
            .iter()
            .find(|plan| plan.expression() == expression)
    }
}
