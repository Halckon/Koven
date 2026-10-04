//! 同轮 unit provider 的共享能力、hidden owner 与严格有序退出。
use super::{UnitDropFact, UnitDropPoint, UnitLoanTarget, UnitOwnershipBindingDescriptor};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitSequentialIterationDescriptor, UnitStatementId},
};

impl super::CompilationUnitOwnership {
    /// 完整provider事实，错误或Deferred时为空，不意味着native已支持。
    #[must_use]
    pub fn iterations(&self) -> &[UnitIterationOwnershipPlan] {
        &self.iterations
    }

    /// source-qualified查询同轮已检查的provider。
    #[must_use]
    pub fn iteration(&self, statement: UnitStatementId) -> Option<&UnitIterationOwnershipPlan> {
        self.iterations
            .iter()
            .find(|plan| plan.descriptor().statement() == statement)
    }

    /// 一个控制点的完整清理序列。存在时消费一次，不能再执行同点普通drops。
    #[must_use]
    pub fn iteration_cleanup_at(
        &self,
        point: UnitDropPoint,
    ) -> Option<&[UnitIterationCleanupAction]> {
        self.iterations
            .iter()
            .flat_map(|plan| plan.exits())
            .find(|exit| exit.point() == point)
            .map(UnitIterationExitPlan::actions)
    }
}

/// source 的现有能力；迭代本身始终只建立 shared access。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitIterationSourceAccess {
    /// owned place 上新建共享借用。
    Owned,
    /// 复用或shared-reborrow既有Borrow能力。
    Shared,
    /// Inout能力上建立shared reborrow，不能升级为独占element。
    Exclusive,
    /// 表达式结果成为延寿的hidden owner。
    Temporary,
}

/// 一个真实可达的退出边；Abort不产生此事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitIterationExitKind {
    /// 完成本轮并保留provider。
    Fallthrough,
    /// 最近loop的continue。
    Continue(UnitExpressionId),
    /// 最近loop的break。
    Break(UnitExpressionId),
    /// 最近callable的return，operand已先交付。
    Return(UnitExpressionId),
    /// 独立的HasNext=false边，包括零轮。
    Exhaustion,
}

/// 按列表顺序执行；同一点多个provider共享同一退出序列，消费一次。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitIterationCleanupAction {
    /// 控制转移放弃的同步call前缀，必须先结束实参loan再结束element/provider。
    EndCallLoan(super::UnitLoanFact),
    /// 已求值member receiver的loan在放弃call时先结束。
    EndReceiverLoan(super::UnitReceiverOwnershipFact),
    /// 结束本轮局部closure的shared capture，随后才能结束其element来源。
    EndCaptureLoan {
        /// 被清理的closure形成身份。
        closure: UnitExpressionId,
        /// 已验证的capture来源。
        source: super::UnitClosureCaptureSource,
    },
    /// 复用同一owner/drop identity，不得再从普通drops重复执行。
    Drop(UnitDropFact),
    /// 结束具名binding及其投影借用。
    EndBinding {
        /// 所属provider。
        statement: UnitStatementId,
        /// 本轮具名element/component。
        symbol: UnitSymbolId,
    },
    /// 结束当前element access，不析构element。
    EndElement(UnitStatementId),
    /// 关闭无分配provider。
    FinishProvider(UnitStatementId),
    /// 结束本层source loan；外部Borrow能力仍属caller。
    EndSource(UnitStatementId),
}

/// 一个退出点的完整顺序；内外provider在return点共享这份序列。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitIterationExitPlan {
    pub(super) kind: UnitIterationExitKind,
    pub(super) point: UnitDropPoint,
    pub(super) actions: Vec<UnitIterationCleanupAction>,
}
impl UnitIterationExitPlan {
    /// 退出控制流种类。
    #[must_use]
    pub const fn kind(&self) -> UnitIterationExitKind {
        self.kind
    }
    /// 普通drop和iteration cleanup共同的执行点。
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        self.point
    }
    /// 该执行点的完整清理序列；不可重排或重复消费。
    #[must_use]
    pub fn actions(&self) -> &[UnitIterationCleanupAction] {
        &self.actions
    }
}

/// 前端验证的provider生命周期；字段不可由消费者构造或修改。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitIterationOwnershipPlan {
    pub(super) descriptor: UnitSequentialIterationDescriptor,
    pub(super) source: UnitLoanTarget,
    pub(super) source_access: UnitIterationSourceAccess,
    pub(super) bindings: Vec<UnitOwnershipBindingDescriptor>,
    pub(super) exits: Vec<UnitIterationExitPlan>,
}
impl UnitIterationOwnershipPlan {
    /// 同轮Phase 2 descriptor。
    #[must_use]
    pub const fn descriptor(&self) -> &UnitSequentialIterationDescriptor {
        &self.descriptor
    }
    /// source的稳定owner/place身份。
    #[must_use]
    pub const fn source(&self) -> &UnitLoanTarget {
        &self.source
    }
    /// source原有能力，不改变provider Shared交付。
    #[must_use]
    pub const fn source_access(&self) -> UnitIterationSourceAccess {
        self.source_access
    }
    /// 每个具名element/component的Shared binding。
    #[must_use]
    pub fn bindings(&self) -> &[UnitOwnershipBindingDescriptor] {
        &self.bindings
    }
    /// 正常与显式跳转的完整计划，Abort没有出口。
    #[must_use]
    pub fn exits(&self) -> &[UnitIterationExitPlan] {
        &self.exits
    }
}
