//! 单文件 nullable `when` 的单次 subject 与逐 edge 类型证明。
use super::TypeId;
use crate::{
    ast::ExpressionId,
    name_resolution::{EnumCaseId, SymbolId},
    source::Span,
};

/// subject 的来源类别；place proof 不授予 owned inner。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullableWhenSubjectCategory {
    /// 可整体交付 Value 的源码 root。
    OwnedRoot,
    /// Borrow 参数或 receiver root。
    BorrowRoot,
    /// Inout 参数或 receiver root。
    InoutRoot,
    /// 普通字段的单次求值 place。
    OrdinaryField,
    /// 容器元素的单次求值 place。
    ContainerElement,
    /// 无源码 root 的临时求值。
    Temporary,
}

/// 可枚举 subject 的闭域原子。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WhenDomainAtom {
    /// Boolean 的一个具体值。
    Boolean(bool),
    /// 仅包含 null。
    Null,
    /// enum 的一个稳定 case。
    Case(EnumCaseId),
}

/// edge 的剩余值域。Finite 保留 enum/Boolean 的精确覆盖顺序。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhenDomain {
    /// 没有可达值。
    Empty,
    /// 仅包含 null。
    Null,
    /// 开放域中的非空值。
    NonNull,
    /// 开放域中的 null 与非空值。
    Nullable,
    /// enum/Boolean 闭域中的有序原子。
    Finite(Vec<WhenDomainAtom>),
}
impl WhenDomain {
    /// 该 edge 是否可达。
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty) || matches!(self, Self::Finite(atoms) if atoms.is_empty())
    }
    /// 每个可达值都非空时才发布 proof；空域不发布证明。
    pub fn is_non_null(&self) -> bool {
        matches!(self, Self::NonNull)
            || matches!(self, Self::Finite(atoms) if !atoms.is_empty() && !atoms.contains(&WhenDomainAtom::Null))
    }
}

/// 按源码顺序发布的 nullable when 类型事实。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullableWhenAlternativeDescriptor {
    pub(crate) span: Span,
    pub(crate) match_domain: WhenDomain,
    pub(crate) fallthrough_domain: WhenDomain,
}
impl NullableWhenAlternativeDescriptor {
    /// 返回该 condition、entry 或 when 关键字的源码范围。
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }
    /// 返回 match_domain 类型事实。
    #[must_use]
    pub fn match_domain(&self) -> &WhenDomain {
        &self.match_domain
    }
    /// 返回 fallthrough_domain 类型事实。
    #[must_use]
    pub fn fallthrough_domain(&self) -> &WhenDomain {
        &self.fallthrough_domain
    }
}

/// 按源码顺序发布的 nullable when 类型事实。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullableWhenEntryDescriptor {
    pub(crate) span: Span,
    pub(crate) input_domain: WhenDomain,
    pub(crate) remaining_domain: WhenDomain,
    pub(crate) alternatives: Vec<NullableWhenAlternativeDescriptor>,
    pub(crate) body_domain: WhenDomain,
    pub(crate) body_type: Option<TypeId>,
    pub(crate) stable_symbol: Option<SymbolId>,
}
impl NullableWhenEntryDescriptor {
    /// 返回该 condition、entry 或 when 关键字的源码范围。
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }
    /// 返回 input_domain 类型事实。
    #[must_use]
    pub fn input_domain(&self) -> &WhenDomain {
        &self.input_domain
    }
    /// 返回 remaining_domain 类型事实。
    #[must_use]
    pub fn remaining_domain(&self) -> &WhenDomain {
        &self.remaining_domain
    }
    /// 返回 alternatives 类型事实。
    #[must_use]
    pub fn alternatives(&self) -> &[NullableWhenAlternativeDescriptor] {
        &self.alternatives
    }
    /// 返回 body_domain 类型事实。
    #[must_use]
    pub fn body_domain(&self) -> &WhenDomain {
        &self.body_domain
    }
    /// 返回 body 入口内部 subject 的非空 inner view 类型；没有共同非空证明时为 None。
    #[must_use]
    pub fn body_type(&self) -> Option<TypeId> {
        self.body_type
    }
    /// 返回仍与内部 subject 对应的稳定源码 binding；place/temporary 没有此身份。
    #[must_use]
    pub fn stable_symbol(&self) -> Option<SymbolId> {
        self.stable_symbol
    }
}

/// 按源码顺序发布的 nullable when 类型事实。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullableWhenDescriptor {
    pub(crate) expression: ExpressionId,
    pub(crate) subject: ExpressionId,
    pub(crate) span: Span,
    pub(crate) subject_type: TypeId,
    pub(crate) category: NullableWhenSubjectCategory,
    pub(crate) stable_symbol: Option<SymbolId>,
    pub(crate) native_eligible: bool,
    pub(crate) entries: Vec<NullableWhenEntryDescriptor>,
}
impl NullableWhenDescriptor {
    /// 返回 expression 类型事实。
    #[must_use]
    pub fn expression(&self) -> ExpressionId {
        self.expression
    }
    /// 返回只求值一次的 subject expression，作为内部证明身份。
    #[must_use]
    pub fn subject(&self) -> ExpressionId {
        self.subject
    }
    /// 返回该 condition、entry 或 when 关键字的源码范围。
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }
    /// 返回 subject_type 类型事实。
    #[must_use]
    pub fn subject_type(&self) -> TypeId {
        self.subject_type
    }
    /// 返回 category 类型事实。
    #[must_use]
    pub fn category(&self) -> NullableWhenSubjectCategory {
        self.category
    }
    /// 返回仍与内部 subject 对应的稳定源码 binding；place/temporary 没有此身份。
    #[must_use]
    pub fn stable_symbol(&self) -> Option<SymbolId> {
        self.stable_symbol
    }
    /// 是否属于 ADR-0017 首版 owned class/Box/Rc nullable handle 子集。
    #[must_use]
    pub fn native_eligible(&self) -> bool {
        self.native_eligible
    }
    /// 返回 entries 类型事实。
    #[must_use]
    pub fn entries(&self) -> &[NullableWhenEntryDescriptor] {
        &self.entries
    }
}
