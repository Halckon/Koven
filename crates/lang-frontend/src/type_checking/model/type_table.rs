//! Stable canonical single-file type identities and interning table.
use super::*;

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
    /// External identity carried only by a scratch call constraint.
    ExternalConstraint(ExternalSymbolId),
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
    /// An argument needs the enclosing call's expected type.
    ContextualArgument,
    /// A pending conditional has a finite set of integer widths.
    ContextualInteger {
        /// Widths available with a concrete numeric expectation.
        widths: u8,
        /// Width available without numeric contextual typing.
        default: Option<BuiltinType>,
        /// At least one result branch requires a nullable value.
        nullable: bool,
    },
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
