//! Map 双 Shared loan 与既有 unit 退出动作共享控制点，不共享 sequential element。
use super::{
    UnitIterationExitPlan, UnitIterationSourceAccess, UnitLoanTarget,
    UnitOwnershipBindingDescriptor,
};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{IterationSource, MapIterationComponent, MapIterationLoan},
    type_checking::{UnitExpressionId, UnitMapIterationDescriptor, UnitStatementId},
};

/// 每个 Map 分量的 source-qualified 独立 loan。
pub type UnitMapIterationLoan =
    MapIterationLoan<UnitStatementId, UnitExpressionId, UnitSymbolId, UnitLoanTarget>;

/// 同轮 Map source lease、两个分量与实际退出。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitMapIterationOwnershipPlan {
    pub(super) descriptor: UnitMapIterationDescriptor,
    pub(super) source: IterationSource<UnitLoanTarget, UnitExpressionId>,
    pub(super) source_access: UnitIterationSourceAccess,
    pub(super) bindings: [UnitOwnershipBindingDescriptor; 2],
    pub(super) loans: [UnitMapIterationLoan; 2],
    pub(super) exits: Vec<UnitIterationExitPlan>,
}
impl UnitMapIterationOwnershipPlan {
    /// 原 Phase 2 Map 事实。
    pub const fn descriptor(&self) -> &UnitMapIterationDescriptor {
        &self.descriptor
    }
    /// 实际 source 的 Shared lease；合法计划恒有来源。
    pub fn source(&self) -> Option<&UnitLoanTarget> {
        self.source.borrowed()
    }
    /// 不可伪装成 owned snapshot。
    pub const fn source_kind(&self) -> &IterationSource<UnitLoanTarget, UnitExpressionId> {
        &self.source
    }
    /// source 原有能力，分量始终只读。
    pub const fn source_access(&self) -> UnitIterationSourceAccess {
        self.source_access
    }
    /// key/value 源码顺序的真实 Shared bindings。
    pub const fn bindings(&self) -> &[UnitOwnershipBindingDescriptor; 2] {
        &self.bindings
    }
    /// 独立身份、共同 provider/source lineage 的双 loan。
    pub const fn loans(&self) -> &[UnitMapIterationLoan; 2] {
        &self.loans
    }
    /// 按分量查询，而非把 key/value 当作字段。
    pub fn loan(&self, component: MapIterationComponent) -> &UnitMapIterationLoan {
        &self.loans[match component {
            MapIterationComponent::Key => 0,
            MapIterationComponent::Value => 1,
        }]
    }
    /// 同一控制点的完整清理序列。
    pub fn exits(&self) -> &[UnitIterationExitPlan] {
        &self.exits
    }
}
impl super::CompilationUnitOwnership {
    /// 完整 Map ownership 计划；错误/deferred 不发布半表。
    pub fn map_iterations(&self) -> &[UnitMapIterationOwnershipPlan] {
        &self.map_iterations
    }
    /// 按 source-qualified For 查询。
    pub fn map_iteration(
        &self,
        statement: UnitStatementId,
    ) -> Option<&UnitMapIterationOwnershipPlan> {
        self.map_iterations
            .iter()
            .find(|plan| plan.descriptor().statement() == statement)
    }
}
