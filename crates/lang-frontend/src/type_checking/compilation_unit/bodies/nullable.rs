use super::{CompilationUnitTypes, UnitExpressionId, UnitTypeId};
use crate::name_resolution::UnitSymbolId;

/// 一个 source-qualified symbol 使用点已由控制流证明为非空。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitNonNullUseDescriptor {
    expression: UnitExpressionId,
    symbol: UnitSymbolId,
    declared_type: UnitTypeId,
    narrowed_type: UnitTypeId,
}

impl UnitNonNullUseDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        symbol: UnitSymbolId,
        declared_type: UnitTypeId,
        narrowed_type: UnitTypeId,
    ) -> Self {
        Self {
            expression,
            symbol,
            declared_type,
            narrowed_type,
        }
    }

    /// 返回被窄化的 source-qualified expression 使用点。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回该使用点解析到的 source-qualified symbol。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回 symbol 的 nullable 声明类型。
    #[must_use]
    pub const fn declared_type(self) -> UnitTypeId {
        self.declared_type
    }

    /// 返回当前 edge 上证明成立的非空 inner type。
    #[must_use]
    pub const fn narrowed_type(self) -> UnitTypeId {
        self.narrowed_type
    }
}

/// 一个 null equality condition 对 source-qualified symbol 建立的 edge fact。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitNullComparisonDescriptor {
    expression: UnitExpressionId,
    symbol: UnitSymbolId,
    nullable_type: UnitTypeId,
    non_null_when_true: bool,
}

impl UnitNullComparisonDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        symbol: UnitSymbolId,
        nullable_type: UnitTypeId,
        non_null_when_true: bool,
    ) -> Self {
        Self {
            expression,
            symbol,
            nullable_type,
            non_null_when_true,
        }
    }

    /// 返回产生 edge fact 的 source-qualified equality expression。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回被比较的 source-qualified symbol。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回 symbol 的 nullable 声明类型。
    #[must_use]
    pub const fn nullable_type(self) -> UnitTypeId {
        self.nullable_type
    }

    /// 返回非空事实是否建立在 condition 的 true edge。
    #[must_use]
    pub const fn non_null_when_true(self) -> bool {
        self.non_null_when_true
    }
}

/// trial 必须整体快照和回滚的 nullable typed facts。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UnitNullableFacts {
    pub(crate) non_null_uses: Vec<UnitNonNullUseDescriptor>,
    pub(crate) null_comparisons: Vec<UnitNullComparisonDescriptor>,
}

impl CompilationUnitTypes {
    /// 返回源码稳定顺序的已证明非空 symbol 使用点。
    #[must_use]
    pub fn non_null_uses(&self) -> &[UnitNonNullUseDescriptor] {
        &self.nullable.non_null_uses
    }

    /// 查询指定 source-qualified expression 的非空使用事实。
    #[must_use]
    pub fn non_null_use(&self, expression: UnitExpressionId) -> Option<UnitNonNullUseDescriptor> {
        self.nullable
            .non_null_uses
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 null equality edge facts。
    #[must_use]
    pub fn null_comparisons(&self) -> &[UnitNullComparisonDescriptor] {
        &self.nullable.null_comparisons
    }

    /// 查询指定 source-qualified equality expression 的 null comparison fact。
    #[must_use]
    pub fn null_comparison(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitNullComparisonDescriptor> {
        self.nullable
            .null_comparisons
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }
}
