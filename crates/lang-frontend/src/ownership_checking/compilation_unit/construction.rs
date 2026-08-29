//! Source-qualified construction ordered-delivery 与 root obligation 产物。

use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitConstructionTarget, UnitExpressionId, UnitTypeId},
};

use super::super::{ConstructionDeliveryKind, ConstructionRootKind};

/// 一次 source-qualified construction Value delivery。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitConstructionDeliveryEffect {
    construction: UnitExpressionId,
    argument: UnitExpressionId,
    parameter_index: usize,
    parameter_symbol: Option<UnitSymbolId>,
    evaluation_index: usize,
    kind: ConstructionDeliveryKind,
}

impl UnitConstructionDeliveryEffect {
    pub(super) const fn new(
        construction: UnitExpressionId,
        argument: UnitExpressionId,
        parameter_index: usize,
        parameter_symbol: Option<UnitSymbolId>,
        evaluation_index: usize,
        kind: ConstructionDeliveryKind,
    ) -> Self {
        Self {
            construction,
            argument,
            parameter_index,
            parameter_symbol,
            evaluation_index,
            kind,
        }
    }

    /// 返回建立新 owner 的 construction expression。
    #[must_use]
    pub const fn construction(self) -> UnitExpressionId {
        self.construction
    }

    /// 返回被求值并交付的 source-qualified operand。
    #[must_use]
    pub const fn argument(self) -> UnitExpressionId {
        self.argument
    }

    /// 返回声明顺序参数下标。
    #[must_use]
    pub const fn parameter_index(self) -> usize {
        self.parameter_index
    }

    /// 返回源码 field/payload symbol；intrinsic 参数为 `None`。
    #[must_use]
    pub const fn parameter_symbol(self) -> Option<UnitSymbolId> {
        self.parameter_symbol
    }

    /// 返回 operand 的源码求值序号。
    #[must_use]
    pub const fn evaluation_index(self) -> usize {
        self.evaluation_index
    }

    /// 返回 copy、move 或 temporary delivery 效果。
    #[must_use]
    pub const fn kind(self) -> ConstructionDeliveryKind {
        self.kind
    }
}

/// construction 成功后沿正常控制流转移或析构的 source-qualified root obligation。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitConstructionRootDropObligation {
    construction: UnitExpressionId,
    result_type: UnitTypeId,
    kind: ConstructionRootKind,
}

impl UnitConstructionRootDropObligation {
    pub(super) const fn new(
        construction: UnitExpressionId,
        result_type: UnitTypeId,
        kind: ConstructionRootKind,
    ) -> Self {
        Self {
            construction,
            result_type,
            kind,
        }
    }

    /// 返回建立 obligation 的 construction expression。
    #[must_use]
    pub const fn construction(self) -> UnitExpressionId {
        self.construction
    }

    /// 返回完整 unit-global construction result type。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.result_type
    }

    /// 返回 inline、heap-owner 或 shared-owner root 表示。
    #[must_use]
    pub const fn kind(self) -> ConstructionRootKind {
        self.kind
    }
}

/// 一个 construction 的 source-qualified ordered-delivery 与 root plan。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstructionOwnershipPlan {
    construction: UnitExpressionId,
    target: UnitConstructionTarget,
    deliveries: Vec<UnitConstructionDeliveryEffect>,
    root_obligation: Option<UnitConstructionRootDropObligation>,
    terminating_operand: Option<UnitExpressionId>,
}

impl UnitConstructionOwnershipPlan {
    pub(super) fn new(
        construction: UnitExpressionId,
        target: UnitConstructionTarget,
        deliveries: Vec<UnitConstructionDeliveryEffect>,
        root_obligation: Option<UnitConstructionRootDropObligation>,
        terminating_operand: Option<UnitExpressionId>,
    ) -> Self {
        Self {
            construction,
            target,
            deliveries,
            root_obligation,
            terminating_operand,
        }
    }

    /// 返回对应的 typed construction expression。
    #[must_use]
    pub const fn construction(&self) -> UnitExpressionId {
        self.construction
    }

    /// 返回稳定 nominal/case/intrinsic target。
    #[must_use]
    pub const fn target(&self) -> UnitConstructionTarget {
        self.target
    }

    /// 返回源码求值顺序的已完成 Value deliveries。
    #[must_use]
    pub fn deliveries(&self) -> &[UnitConstructionDeliveryEffect] {
        &self.deliveries
    }

    /// 返回 MoveOnly root obligation；Copyable 或提前终止时为 `None`。
    #[must_use]
    pub const fn root_obligation(&self) -> Option<UnitConstructionRootDropObligation> {
        self.root_obligation
    }

    /// 返回阻止 construction 完成的 `Nothing` operand。
    #[must_use]
    pub const fn terminating_operand(&self) -> Option<UnitExpressionId> {
        self.terminating_operand
    }
}
