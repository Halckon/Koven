//! Canonical compilation-unit type identities, checkpointing and interning.
use super::*;

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
    /// 组合参数模式与既有类型身份，供 canonical 结构查询使用；不发布类型事实。
    #[must_use]
    pub const fn new(mode: ParameterMode, ty: UnitTypeId) -> Self {
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

    pub(in crate::type_checking::compilation_unit) fn discard_after(&mut self, checkpoint: usize) {
        self.canonical.discard_after(checkpoint);
    }

    pub(in crate::type_checking::compilation_unit) fn take_after(
        &mut self,
        checkpoint: usize,
    ) -> Vec<UnitTypeKind> {
        self.canonical.take_after(checkpoint)
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
