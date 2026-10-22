//! 同轮 nullable When 的真实分支结果与 direct-call 参数提取。
use super::{CompilationUnitOwnership, UnitLoanFact, UnitOwnershipPlace, UnitValueDeliveryFact};
use crate::{
    source::Span,
    type_checking::{
        UnitExpressionId, UnitNullableWhenDescriptor, UnitNullableWhenUseWitness, UnitStatementId,
    },
};

/// 已完成的真实参数契约，不是按 expected type 推测的分支汇总。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitNullableWhenUseKind {
    /// 同步 Shared loan 观察，不取得 nullable inner 的 owner。
    Observe,
    /// Value 参数实际移动整个 nullable root，唯一 inner 由调用接管。
    Consume,
}

/// 最外实参仅一条事实；透明链的 canonical reference 保留在 typed witness 中。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNullableWhenUseFact {
    pub(super) call: UnitExpressionId,
    pub(super) argument: UnitExpressionId,
    pub(super) witness: UnitNullableWhenUseWitness,
    pub(super) kind: UnitNullableWhenUseKind,
    pub(super) delivery: Option<UnitValueDeliveryFact>,
    pub(super) loan: Option<UnitLoanFact>,
    pub(super) moved_span: Option<Span>,
}

impl UnitNullableWhenUseFact {
    /// 已应用真实 ownership contract 的 call identity。
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }

    /// 真实 call 参数，包括源码最外层 Group。
    #[must_use]
    pub const fn argument(&self) -> UnitExpressionId {
        self.argument
    }

    /// 透明链底部 Name 的 source-qualified identity。
    #[must_use]
    pub const fn reference(&self) -> UnitExpressionId {
        self.witness.reference()
    }

    /// 同轮 typed control/subject/entry/generation 来源凭据。
    #[must_use]
    pub const fn witness(&self) -> UnitNullableWhenUseWitness {
        self.witness
    }

    /// 实际 shared observation 或 whole-root move。
    #[must_use]
    pub const fn kind(&self) -> UnitNullableWhenUseKind {
        self.kind
    }

    /// Consume 引用的真实 Value delivery；Observe 没有 delivery。
    #[must_use]
    pub const fn delivery(&self) -> Option<&UnitValueDeliveryFact> {
        self.delivery.as_ref()
    }

    /// Observe 引用的真实 Shared loan；Consume 没有 loan。
    #[must_use]
    pub const fn loan(&self) -> Option<&UnitLoanFact> {
        self.loan.as_ref()
    }

    /// 原 consume_place 成功后 moved[root] 的 canonical Name span。
    #[must_use]
    pub const fn moved_span(&self) -> Option<Span> {
        self.moved_span
    }
}

/// 真实 ownership body traversal 的后继，不以 typed 预期覆盖实际结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitNullableWhenEntryOwnershipFact {
    pub(super) index: usize,
    pub(super) body: UnitStatementId,
    pub(super) falls_through: bool,
    pub(super) breaks: bool,
    pub(super) continues: bool,
}

impl UnitNullableWhenEntryOwnershipFact {
    /// 原 When entry 下标。
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    /// 实际检查的 source-qualified body。
    #[must_use]
    pub const fn body(self) -> UnitStatementId {
        self.body
    }

    /// body 是否产生正常 next state。
    #[must_use]
    pub const fn falls_through(self) -> bool {
        self.falls_through
    }

    /// body 是否产生离开当前 loop 的 break state。
    #[must_use]
    pub const fn breaks(self) -> bool {
        self.breaks
    }

    /// body 是否产生当前 loop 的 continue state。
    #[must_use]
    pub const fn continues(self) -> bool {
        self.continues
    }
}

/// 来自原 typed/owned analysis owner 的完整 When 计划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNullableWhenOwnershipPlan {
    pub(super) descriptor: UnitNullableWhenDescriptor,
    pub(super) source_place: Option<UnitOwnershipPlace>,
    pub(super) entries: Vec<UnitNullableWhenEntryOwnershipFact>,
    pub(super) uses: Vec<UnitNullableWhenUseFact>,
}

impl UnitNullableWhenOwnershipPlan {
    /// 已检查的控制、subject、类型、域和 generation 身份。
    #[must_use]
    pub fn descriptor(&self) -> &UnitNullableWhenDescriptor {
        &self.descriptor
    }

    /// 实际来源 place；temporary 无 symbol，不伪造 root。
    #[must_use]
    pub fn source_place(&self) -> Option<&UnitOwnershipPlace> {
        self.source_place.as_ref()
    }

    /// 按真实 entry 下标排序的正常/退出结果。
    #[must_use]
    pub fn entries(&self) -> &[UnitNullableWhenEntryOwnershipFact] {
        &self.entries
    }

    /// 按 call/最外 argument identity 排序的已执行契约。
    #[must_use]
    pub fn uses(&self) -> &[UnitNullableWhenUseFact] {
        &self.uses
    }
}

impl CompilationUnitOwnership {
    /// 同一 analysis 内的完整 nullable When ownership plans。
    #[must_use]
    pub fn nullable_whens(&self) -> &[UnitNullableWhenOwnershipPlan] {
        &self.nullable_whens
    }

    /// 查询真实 source-qualified control；不接受 Single 或同名替代。
    #[must_use]
    pub fn nullable_when(
        &self,
        control: UnitExpressionId,
    ) -> Option<&UnitNullableWhenOwnershipPlan> {
        self.nullable_whens
            .binary_search_by_key(&control, |plan| plan.descriptor.expression())
            .ok()
            .map(|index| &self.nullable_whens[index])
    }

    /// 查询一个真实最外 call argument 的唯一提取契约。
    #[must_use]
    pub fn nullable_when_use(&self, argument: UnitExpressionId) -> Option<&UnitNullableWhenUseFact> {
        self.nullable_whens
            .iter()
            .flat_map(|plan| &plan.uses)
            .find(|fact| fact.argument == argument)
    }
}
