use crate::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    type_checking::{ConstructionTarget, TypeId},
};

/// construction operand 完成求值后交付给新 owner 的方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructionDeliveryKind {
    /// 从 `Copyable` place 建立独立 owned copy。
    Copy,
    /// 从 MoveOnly place 转移唯一 owner。
    Move,
    /// 把本次求值产生的 temporary 直接交付给 construction。
    DeliverTemporary,
}

/// 一次源码有序的 construction Value delivery。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructionDeliveryEffect {
    construction: ExpressionId,
    argument: ExpressionId,
    parameter_index: usize,
    parameter_symbol: Option<SymbolId>,
    evaluation_index: usize,
    kind: ConstructionDeliveryKind,
}

impl ConstructionDeliveryEffect {
    pub(crate) const fn new(
        construction: ExpressionId,
        argument: ExpressionId,
        parameter_index: usize,
        parameter_symbol: Option<SymbolId>,
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
    pub const fn construction(self) -> ExpressionId {
        self.construction
    }

    /// 返回被求值并交付的源码 operand。
    #[must_use]
    pub const fn argument(self) -> ExpressionId {
        self.argument
    }

    /// 返回参数声明序号。
    #[must_use]
    pub const fn parameter_index(self) -> usize {
        self.parameter_index
    }

    /// 返回源码字段或 payload symbol；intrinsic 参数为 `None`。
    #[must_use]
    pub const fn parameter_symbol(self) -> Option<SymbolId> {
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

/// construction 建立的 MoveOnly root 表示类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructionRootKind {
    /// value class 或 enum 的内联 owner。
    Inline,
    /// ordinary class 或 intrinsic `Box` 的唯一 heap-owner handle。
    HeapOwner,
}

/// construction 成功后必须沿正常控制流唯一转移或析构的 root obligation。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructionRootDropObligation {
    construction: ExpressionId,
    result_type: TypeId,
    kind: ConstructionRootKind,
}

impl ConstructionRootDropObligation {
    pub(crate) const fn new(
        construction: ExpressionId,
        result_type: TypeId,
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
    pub const fn construction(self) -> ExpressionId {
        self.construction
    }

    /// 返回完整单态 construction result type。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回 inline 或 heap-owner root 表示。
    #[must_use]
    pub const fn kind(self) -> ConstructionRootKind {
        self.kind
    }
}

/// 一个 construction 的完整 Phase 3 ordered-delivery 与 root plan。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionOwnershipPlan {
    construction: ExpressionId,
    target: ConstructionTarget,
    deliveries: Vec<ConstructionDeliveryEffect>,
    root_obligation: Option<ConstructionRootDropObligation>,
    terminating_operand: Option<ExpressionId>,
}

impl ConstructionOwnershipPlan {
    pub(crate) fn new(
        construction: ExpressionId,
        target: ConstructionTarget,
        deliveries: Vec<ConstructionDeliveryEffect>,
        root_obligation: Option<ConstructionRootDropObligation>,
        terminating_operand: Option<ExpressionId>,
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
    pub const fn construction(&self) -> ExpressionId {
        self.construction
    }

    /// 返回稳定 nominal/case/intrinsic target。
    #[must_use]
    pub const fn target(&self) -> ConstructionTarget {
        self.target
    }

    /// 返回源码求值顺序的已完成 Value deliveries。
    #[must_use]
    pub fn deliveries(&self) -> &[ConstructionDeliveryEffect] {
        &self.deliveries
    }

    /// 返回 MoveOnly root obligation；全 Copyable 或提前终止时为 `None`。
    #[must_use]
    pub const fn root_obligation(&self) -> Option<ConstructionRootDropObligation> {
        self.root_obligation
    }

    /// 返回阻止 construction 完成的 `Nothing` operand。
    #[must_use]
    pub const fn terminating_operand(&self) -> Option<ExpressionId> {
        self.terminating_operand
    }
}
