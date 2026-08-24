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
    AggregateProjectionDescriptor, CallDescriptor, ContainerConstructionDescriptor,
    ElementPlaceDescriptor, ExpressionCategory, FunctionParameterType, IntrinsicCallable,
    ParameterBindingDescriptor, ParameterMode,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelegationPlan {
    pub(crate) owner: NominalId,
    pub(crate) interface: TypeId,
    pub(crate) target: SymbolId,
}

/// 已规范化的顶层或实例 member callable 签名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableDescriptor {
    pub(crate) symbol: SymbolId,
    pub(crate) owner: Option<NominalId>,
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
    pub const fn owner(self) -> NominalId {
        self.owner
    }
    /// 返回完整 invariant interface 实例。
    #[must_use]
    pub const fn interface(self) -> TypeId {
        self.interface
    }
    /// 返回同一主构造器的 immutable field symbol。
    #[must_use]
    pub const fn target(self) -> SymbolId {
        self.target
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

/// 插入顺序稳定并按结构去重的类型表。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTable {
    kinds: Vec<TypeKind>,
    ids: BTreeMap<TypeKind, TypeId>,
}

impl TypeTable {
    pub(crate) fn new() -> Self {
        let mut table = Self {
            kinds: Vec::new(),
            ids: BTreeMap::new(),
        };
        for builtin in [
            BuiltinType::Byte,
            BuiltinType::Short,
            BuiltinType::Int,
            BuiltinType::Long,
            BuiltinType::UByte,
            BuiltinType::UShort,
            BuiltinType::UInt,
            BuiltinType::ULong,
            BuiltinType::Float,
            BuiltinType::Double,
            BuiltinType::Boolean,
            BuiltinType::Char,
            BuiltinType::String,
            BuiltinType::Unit,
            BuiltinType::Nothing,
            BuiltinType::Any,
        ] {
            table.intern(TypeKind::Builtin(builtin));
        }
        table.intern(TypeKind::IntegerLiteral(IntegerConstraint::Signed));
        table.intern(TypeKind::IntegerLiteral(IntegerConstraint::Unsigned));
        table.intern(TypeKind::Error);
        table
    }

    pub(crate) fn intern(&mut self, kind: TypeKind) -> TypeId {
        if let Some(id) = self.ids.get(&kind).copied() {
            return id;
        }
        let id = TypeId(self.kinds.len());
        self.kinds.push(kind.clone());
        self.ids.insert(kind, id);
        id
    }

    /// 按身份读取规范化结构。
    #[must_use]
    pub fn get(&self, id: TypeId) -> Option<&TypeKind> {
        self.kinds.get(id.index())
    }

    /// 查询本次 typed 产物中的内建类型身份。
    #[must_use]
    pub fn builtin(&self, builtin: BuiltinType) -> Option<TypeId> {
        self.ids.get(&TypeKind::Builtin(builtin)).copied()
    }

    /// 返回类型表大小。
    #[must_use]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// 返回类型表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

/// Phase 2 的单文件 typed 产物。
#[derive(Clone, Debug)]
pub struct TypedFile {
    source_id: SourceId,
    environment_owner: Arc<()>,
    name_analysis_owner: Arc<()>,
    analysis_owner: Arc<()>,
    types: TypeTable,
    expression_types: Vec<TypeId>,
    type_ref_types: Vec<TypeId>,
    symbol_types: Vec<TypeId>,
    parameter_bindings: Vec<ParameterBindingDescriptor>,
    nominals: Vec<NominalDescriptor>,
    type_parameters: Vec<TypeParameterDescriptor>,
    delegations: Vec<DelegationPlan>,
    callables: Vec<CallableDescriptor>,
    enum_cases: Vec<EnumCaseDescriptor>,
    copyabilities: Vec<Copyability>,
    destructurings: Vec<DestructuringDescriptor>,
    expression_categories: Vec<ExpressionCategory>,
    calls: Vec<CallDescriptor>,
    aggregate_projections: Vec<AggregateProjectionDescriptor>,
    pub(crate) container_constructions: Vec<ContainerConstructionDescriptor>,
    pub(crate) element_places: Vec<ElementPlaceDescriptor>,
    diagnostics: Vec<Diagnostic>,
}

pub(crate) struct TypedFileParts {
    pub(crate) expression_types: Vec<TypeId>,
    pub(crate) type_ref_types: Vec<TypeId>,
    pub(crate) symbol_types: Vec<TypeId>,
    pub(crate) parameter_bindings: Vec<ParameterBindingDescriptor>,
    pub(crate) nominals: Vec<NominalDescriptor>,
    pub(crate) type_parameters: Vec<TypeParameterDescriptor>,
    pub(crate) delegations: Vec<DelegationPlan>,
    pub(crate) callables: Vec<CallableDescriptor>,
    pub(crate) enum_cases: Vec<EnumCaseDescriptor>,
    pub(crate) copyabilities: Vec<Copyability>,
    pub(crate) destructurings: Vec<DestructuringDescriptor>,
    pub(crate) expression_categories: Vec<ExpressionCategory>,
    pub(crate) calls: Vec<CallDescriptor>,
    pub(crate) aggregate_projections: Vec<AggregateProjectionDescriptor>,
    pub(crate) container_constructions: Vec<ContainerConstructionDescriptor>,
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
            source_id,
            environment_owner,
            name_analysis_owner,
            analysis_owner: Arc::new(()),
            types,
            expression_types: parts.expression_types,
            type_ref_types: parts.type_ref_types,
            symbol_types: parts.symbol_types,
            parameter_bindings: parts.parameter_bindings,
            nominals: parts.nominals,
            type_parameters: parts.type_parameters,
            delegations: parts.delegations,
            callables: parts.callables,
            enum_cases: parts.enum_cases,
            copyabilities: parts.copyabilities,
            destructurings: parts.destructurings,
            expression_categories: parts.expression_categories,
            calls: parts.calls,
            aggregate_projections: parts.aggregate_projections,
            container_constructions: parts.container_constructions,
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

    #[must_use]
    /// 返回稳定排序的类型诊断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
