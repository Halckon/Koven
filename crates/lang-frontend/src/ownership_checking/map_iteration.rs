//! Map 的双分量借用身份；不构造顺序 element、entry owner 或字段投影。
use super::{
    IterationCaptureGraph, IterationClosureFlow, IterationClosurePhiBinding, IterationExitPlan,
    IterationPhiIncoming, IterationSource, LoanKind, LoanTarget, OwnershipBindingDescriptor,
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    type_checking::MapIterationDescriptor,
};

/// 同一 Map provider 当前轮次中的两个独立只读分量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapIterationComponent {
    /// 源码解构的第一个 binding。
    Key,
    /// 源码解构的第二个 binding。
    Value,
}

/// 一个 source-qualified 分量 loan；两分量共享来源 lease，但不共享 loan 身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapIterationLoan<St = StatementId, E = ExpressionId, S = SymbolId, L = LoanTarget> {
    pub(crate) statement: St,
    pub(crate) source: E,
    pub(crate) component: MapIterationComponent,
    pub(crate) binding: S,
    pub(crate) origin: L,
}
impl<St: Copy, E: Copy, S: Copy, L> MapIterationLoan<St, E, S, L> {
    /// 建立本轮分量的 provider identity。
    #[must_use]
    pub const fn statement(&self) -> St {
        self.statement
    }
    /// 唯一求值的完整 source 表达式。
    #[must_use]
    pub const fn source(&self) -> E {
        self.source
    }
    /// 独立 Key/Value 身份，不是 value-class field。
    #[must_use]
    pub const fn component(&self) -> MapIterationComponent {
        self.component
    }
    /// 真实 names binding identity。
    #[must_use]
    pub const fn binding(&self) -> S {
        self.binding
    }
    /// 该 provider 实际借用的 source owner/place。
    #[must_use]
    pub const fn origin(&self) -> &L {
        &self.origin
    }
    /// Map 分量恒为 Shared，没有 owning/独占交付。
    #[must_use]
    pub const fn kind(&self) -> LoanKind {
        LoanKind::Shared
    }
}

/// Source 原有能力；provider 始终只建立 Shared reborrow。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapIterationSourceAccess {
    /// 具名或投影到 owned place。
    Owned,
    /// Borrow 参数、借用结果绑定或外层 Map 分量。
    Shared,
    /// Inout place 的 Shared reborrow。
    Exclusive,
    /// 唯一求值产生的 hidden owner 延寿到 provider 结束。
    Temporary,
}

/// Map Phase 3 完整事实；独立于 SequentialIterationDescriptor。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapIterationOwnershipPlan {
    pub(crate) descriptor: MapIterationDescriptor,
    pub(crate) source: IterationSource<LoanTarget, ExpressionId>,
    pub(crate) source_access: MapIterationSourceAccess,
    pub(crate) bindings: [OwnershipBindingDescriptor; 2],
    pub(crate) loans: [MapIterationLoan; 2],
    pub(crate) exits: Vec<IterationExitPlan>,
    pub(crate) closure_flow: IterationClosureFlow,
    pub(crate) capture_graph: IterationCaptureGraph,
    pub(crate) closure_phis: Vec<IterationClosurePhiBinding>,
    pub(crate) closure_phi_incomings: Vec<IterationPhiIncoming>,
}
impl MapIterationOwnershipPlan {
    /// 同轮已验证的 Phase 2 source/K/V 身份。
    #[must_use]
    pub const fn descriptor(&self) -> &MapIterationDescriptor {
        &self.descriptor
    }
    /// 覆盖 provider 生命周期的 Shared source lease。
    #[must_use]
    pub fn source(&self) -> Option<&LoanTarget> {
        self.source.borrowed()
    }
    /// 保留明确的借用来源种类；Map 不发布 OwnedSnapshot。
    #[must_use]
    pub const fn source_kind(&self) -> &IterationSource<LoanTarget, ExpressionId> {
        &self.source
    }
    /// Source 原有能力，不授予 binding 独占能力。
    #[must_use]
    pub const fn source_access(&self) -> MapIterationSourceAccess {
        self.source_access
    }
    /// Key、Value 源码顺序的两个 Shared binding。
    #[must_use]
    pub const fn bindings(&self) -> &[OwnershipBindingDescriptor; 2] {
        &self.bindings
    }
    /// Key、Value 源码顺序的两个独立 source-qualified loan。
    #[must_use]
    pub const fn loans(&self) -> &[MapIterationLoan; 2] {
        &self.loans
    }
    /// 按分量身份查询该轮 Shared loan。
    #[must_use]
    pub fn loan(&self, component: MapIterationComponent) -> &MapIterationLoan {
        &self.loans[match component {
            MapIterationComponent::Key => 0,
            MapIterationComponent::Value => 1,
        }]
    }
    /// 每一实际出口的完整同点清理；混合嵌套亦只消费一次。
    #[must_use]
    pub fn exits(&self) -> &[IterationExitPlan] {
        &self.exits
    }
    /// 共用循环 control-flow 容器，不把 Map 分量注册为 owner。
    #[must_use]
    pub const fn closure_flow(&self) -> &IterationClosureFlow {
        &self.closure_flow
    }
    /// 本循环的有限捕获图。
    #[must_use]
    pub const fn capture_graph(&self) -> &IterationCaptureGraph {
        &self.capture_graph
    }
    /// 正常循环 owner 的预分配 phi。
    #[must_use]
    pub fn closure_phis(&self) -> &[IterationClosurePhiBinding] {
        &self.closure_phis
    }
    /// 实际控制流入边。
    #[must_use]
    pub fn closure_phi_incomings(&self) -> &[IterationPhiIncoming] {
        &self.closure_phi_incomings
    }
}

/// 同一次实际 drop traversal 记录的 required exit；不从 AST arena 推断。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapIterationRequiredExit {
    pub(crate) statement: StatementId,
    pub(crate) kind: super::IterationExitKind,
    pub(crate) point: super::DropPoint,
    pub(crate) condition: super::CleanupConditionId,
}
