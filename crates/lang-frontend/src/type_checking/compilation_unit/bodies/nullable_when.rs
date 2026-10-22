//! 同一 compilation-unit 分析内的 nullable null/else 控制与使用来源。

use super::CompilationUnitTypes;
use crate::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{
        NullableWhenSubjectCategory, UnitExpressionId, UnitStatementId, UnitTypeId,
    },
};

/// 已计算 When 域的 nullability 投影，不携带单文件 enum identity。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitNullableWhenDomain {
    /// 没有可达值。
    Empty,
    /// 仅有 null。
    Null,
    /// 仅有非空值。
    NonNull,
    /// null 与非空值均可能存在。
    Nullable,
}

/// 一个真实 null condition 的输入、匹配和剩余域。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitNullableWhenAlternativeDescriptor {
    pub(super) expression: UnitExpressionId,
    pub(super) span: Span,
    pub(super) input_domain: UnitNullableWhenDomain,
    pub(super) matched_domain: UnitNullableWhenDomain,
    pub(super) remaining_domain: UnitNullableWhenDomain,
}

impl UnitNullableWhenAlternativeDescriptor {
    /// 真实 condition 表达式。
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }
    /// condition 的源码范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// condition 接收的域。
    #[must_use]
    pub const fn input_domain(&self) -> UnitNullableWhenDomain {
        self.input_domain
    }
    /// condition 匹配 edge 的域。
    #[must_use]
    pub const fn matched_domain(&self) -> UnitNullableWhenDomain {
        self.matched_domain
    }
    /// condition 未匹配 edge 的域。
    #[must_use]
    pub const fn remaining_domain(&self) -> UnitNullableWhenDomain {
        self.remaining_domain
    }
}

/// 一个实际 entry 的共同 body 域与完成状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNullableWhenEntryDescriptor {
    pub(super) index: usize,
    pub(super) body: UnitStatementId,
    pub(super) span: Span,
    pub(super) is_else: bool,
    pub(super) input_domain: UnitNullableWhenDomain,
    pub(super) body_domain: UnitNullableWhenDomain,
    pub(super) remaining_domain: UnitNullableWhenDomain,
    pub(super) alternatives: Vec<UnitNullableWhenAlternativeDescriptor>,
    pub(super) falls_through: bool,
}

impl UnitNullableWhenEntryDescriptor {
    /// entry 在控制中的源码顺序。
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
    /// 真实 entry body 身份。
    #[must_use]
    pub const fn body(&self) -> UnitStatementId {
        self.body
    }
    /// 完整 entry 的源码范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// 是否为实际 else entry。
    #[must_use]
    pub const fn is_else(&self) -> bool {
        self.is_else
    }
    /// entry 接收的域。
    #[must_use]
    pub const fn input_domain(&self) -> UnitNullableWhenDomain {
        self.input_domain
    }
    /// 所有可达 alternative 共同 body 域。
    #[must_use]
    pub const fn body_domain(&self) -> UnitNullableWhenDomain {
        self.body_domain
    }
    /// entry 后的剩余域。
    #[must_use]
    pub const fn remaining_domain(&self) -> UnitNullableWhenDomain {
        self.remaining_domain
    }
    /// 实际 condition 的源码顺序事实。
    #[must_use]
    pub fn alternatives(&self) -> &[UnitNullableWhenAlternativeDescriptor] {
        &self.alternatives
    }
    /// body 检查所得正常完成状态。
    #[must_use]
    pub const fn falls_through(&self) -> bool {
        self.falls_through
    }
}

/// nullable null/else 的单次 subject 与真实分支事实；不授 native 资格。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNullableWhenDescriptor {
    pub(super) expression: UnitExpressionId,
    pub(super) subject: UnitExpressionId,
    pub(super) nullable_type: UnitTypeId,
    pub(super) inner_type: UnitTypeId,
    pub(super) source_category: NullableWhenSubjectCategory,
    pub(super) stable_symbol: Option<UnitSymbolId>,
    pub(super) subject_reference: Option<UnitExpressionId>,
    pub(super) source_generation: Option<u64>,
    pub(super) entries: Vec<UnitNullableWhenEntryDescriptor>,
    pub(super) falls_through: bool,
}

impl UnitNullableWhenDescriptor {
    /// 实际 When control 身份。
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }
    /// 只求值一次的 subject 身份。
    #[must_use]
    pub const fn subject(&self) -> UnitExpressionId {
        self.subject
    }
    /// subject 检查所得 nullable 类型。
    #[must_use]
    pub const fn nullable_type(&self) -> UnitTypeId {
        self.nullable_type
    }
    /// nullable wrapper 的 inner 类型。
    #[must_use]
    pub const fn inner_type(&self) -> UnitTypeId {
        self.inner_type
    }
    /// subject 的真实 root/place/temporary 类别。
    #[must_use]
    pub const fn source_category(&self) -> NullableWhenSubjectCategory {
        self.source_category
    }
    /// 稳定来源 symbol；temporary 和 This 不伪造 symbol。
    #[must_use]
    pub const fn stable_symbol(&self) -> Option<UnitSymbolId> {
        self.stable_symbol
    }
    /// Name/Group subject 的 canonical Name；不是 use reference。
    #[must_use]
    pub const fn subject_reference(&self) -> Option<UnitExpressionId> {
        self.subject_reference
    }
    /// subject 求值时稳定 symbol 的 flow generation。
    #[must_use]
    pub const fn source_generation(&self) -> Option<u64> {
        self.source_generation
    }
    /// 真实 entry 的源码顺序事实。
    #[must_use]
    pub fn entries(&self) -> &[UnitNullableWhenEntryDescriptor] {
        &self.entries
    }
    /// 完整 When 是否存在正常后继。
    #[must_use]
    pub const fn falls_through(&self) -> bool {
        self.falls_through
    }
}

/// 真实 Name/Group use 与当前 When edge、source generation 的联合来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitNullableWhenUseWitness {
    pub(super) expression: UnitExpressionId,
    pub(super) reference: UnitExpressionId,
    pub(super) control: UnitExpressionId,
    pub(super) subject: UnitExpressionId,
    pub(super) entry_index: usize,
    pub(super) body: UnitStatementId,
    pub(super) symbol: UnitSymbolId,
    pub(super) source_generation: u64,
    pub(super) nullable_type: UnitTypeId,
    pub(super) inner_type: UnitTypeId,
}

impl UnitNullableWhenUseWitness {
    /// 该次真实 Name 或透明 Group 使用点。
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }
    /// 同次 Group 链唯一的 canonical Name。
    #[must_use]
    pub const fn reference(&self) -> UnitExpressionId {
        self.reference
    }
    /// 建立当前证明的实际 When。
    #[must_use]
    pub const fn control(&self) -> UnitExpressionId {
        self.control
    }
    /// 与当前证明绑定的单次 subject。
    #[must_use]
    pub const fn subject(&self) -> UnitExpressionId {
        self.subject
    }
    /// 当前 entry 的源码顺序。
    #[must_use]
    pub const fn entry_index(&self) -> usize {
        self.entry_index
    }
    /// 建立当前证明的真实 body。
    #[must_use]
    pub const fn body(&self) -> UnitStatementId {
        self.body
    }
    /// 使用点实际解析的稳定 symbol。
    #[must_use]
    pub const fn symbol(&self) -> UnitSymbolId {
        self.symbol
    }
    /// 与 subject 相同且未失效的 flow generation。
    #[must_use]
    pub const fn source_generation(&self) -> u64 {
        self.source_generation
    }
    /// 来源 wrapper 类型。
    #[must_use]
    pub const fn nullable_type(&self) -> UnitTypeId {
        self.nullable_type
    }
    /// 当前非空 fact 与使用点一致的 inner 类型。
    #[must_use]
    pub const fn inner_type(&self) -> UnitTypeId {
        self.inner_type
    }
}

impl CompilationUnitTypes {
    /// 返回同一分析内、按 source-qualified control 排序的 null/else 描述符。
    #[must_use]
    pub fn nullable_whens(&self) -> &[UnitNullableWhenDescriptor] {
        &self.nullable.whens
    }

    /// 查询实际 When control；descriptor 本身不授 native 能力。
    #[must_use]
    pub fn nullable_when(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitNullableWhenDescriptor> {
        self.nullable
            .whens
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回实际 Name/Group 使用点的 When 来源，而非一般非空事实。
    #[must_use]
    pub fn nullable_when_uses(&self) -> &[UnitNullableWhenUseWitness] {
        &self.nullable.when_uses
    }

    /// 查询当前分析发布的精确使用点 witness；不按 symbol/type 回退。
    #[must_use]
    pub fn nullable_when_use(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitNullableWhenUseWitness> {
        self.nullable
            .when_uses
            .iter()
            .copied()
            .find(|witness| witness.expression() == expression)
    }
}
