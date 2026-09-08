//! nullable when 的非 owning proof 与显式 inner 交付事实。
use super::{DropFact, OwnershipPlace};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    type_checking::{NullableWhenSubjectCategory, TypeId},
};

/// 非空 inner 的 Value 交付方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullableWhenExtractionKind {
    /// 复制 Copyable inner，保留 nullable source。
    Copy,
    /// 消费整个合法 nullable owner，交付唯一 inner。
    Consume,
}
/// 分支汇总；MayConsume 不声称每条内部路径都执行 take。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullableWhenBranchOutcome {
    /// 入口值域仅为 null，且存在正常后继。
    Null,
    /// 尚无 Consume；允许存在保持源完整的 Copy。
    Unextracted,
    /// 至少一条已检查路径发生 Consume，不表示每条路径都消费。
    MayConsume,
    /// 没有正常后继；具体清理见 control-transfer drop facts。
    Diverging,
}

/// 仅由成功的所有权分析发布，保留 typed subject 与源码 edge 身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullableWhenProofView {
    pub(crate) subject: ExpressionId,
    pub(crate) place: Option<OwnershipPlace>,
    pub(crate) stable_symbol: Option<SymbolId>,
    pub(crate) inner_type: TypeId,
}
impl NullableWhenProofView {
    /// 返回只求值一次的内部 subject 身份；不对应重新读取的 field/element。
    #[must_use]
    pub fn subject(&self) -> ExpressionId {
        self.subject
    }
    /// 返回该次求值所依附的 root/field/element identity；temporary 为 None。
    #[must_use]
    pub fn place(&self) -> Option<&OwnershipPlace> {
        self.place.as_ref()
    }
    /// 返回可复用证明的稳定源码绑定；field/element 与 temporary 为 None。
    #[must_use]
    pub fn stable_symbol(&self) -> Option<SymbolId> {
        self.stable_symbol
    }
    /// 返回该 edge 已证明非空的 inner 类型。
    #[must_use]
    pub fn inner_type(&self) -> TypeId {
        self.inner_type
    }
}

/// 仅由成功的所有权分析发布，保留 typed subject 与源码 edge 身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullableWhenExtractionFact {
    pub(crate) expression: ExpressionId,
    pub(crate) entry: usize,
    pub(crate) alternative: Option<usize>,
    pub(crate) kind: NullableWhenExtractionKind,
}
impl NullableWhenExtractionFact {
    /// 条件中的源码 alternative 下标；body 内交付为 None。
    pub fn alternative(&self) -> Option<usize> {
        self.alternative
    }

    /// 返回产生该事实的源码表达式身份。
    #[must_use]
    pub fn expression(&self) -> ExpressionId {
        self.expression
    }
    /// 返回所属 when entry 的源码顺序下标。
    #[must_use]
    pub fn entry(&self) -> usize {
        self.entry
    }
    /// 返回复制 inner 或消费整个 nullable owner 的交付方式。
    #[must_use]
    pub fn kind(&self) -> NullableWhenExtractionKind {
        self.kind
    }
}

/// 仅由成功的所有权分析发布，保留 typed subject 与源码 edge 身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullableWhenBranchFact {
    pub(crate) index: usize,
    pub(crate) view: Option<NullableWhenProofView>,
    pub(crate) outcome: NullableWhenBranchOutcome,
    pub(crate) drops: Vec<DropFact>,
}
impl NullableWhenBranchFact {
    /// 返回该 branch 对应的源码 entry 下标。
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }
    /// 返回非 owning 的共同非空证明；不产生 copy、retain 或析构义务。
    #[must_use]
    pub fn view(&self) -> Option<&NullableWhenProofView> {
        self.view.as_ref()
    }
    /// 返回 branch 正常后继及可能消费的保守汇总。
    #[must_use]
    pub fn outcome(&self) -> NullableWhenBranchOutcome {
        self.outcome
    }
    /// 返回本 entry 内及其匹配/退出 edge 的已有精确析构事实。
    #[must_use]
    pub fn drops(&self) -> &[DropFact] {
        &self.drops
    }
}

/// 仅由成功的所有权分析发布，保留 typed subject 与源码 edge 身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullableWhenOwnershipPlan {
    pub(crate) expression: ExpressionId,
    pub(crate) subject: ExpressionId,
    pub(crate) category: NullableWhenSubjectCategory,
    pub(crate) branches: Vec<NullableWhenBranchFact>,
    pub(crate) extractions: Vec<NullableWhenExtractionFact>,
}
impl NullableWhenOwnershipPlan {
    /// 返回产生该事实的源码表达式身份。
    #[must_use]
    pub fn expression(&self) -> ExpressionId {
        self.expression
    }
    /// 返回只求值一次的内部 subject 身份；不对应重新读取的 field/element。
    #[must_use]
    pub fn subject(&self) -> ExpressionId {
        self.subject
    }
    /// 返回 typed subject 的来源能力类别，不授予额外移动权限。
    #[must_use]
    pub fn category(&self) -> NullableWhenSubjectCategory {
        self.category
    }
    /// 返回按源码 entry 顺序排列的 branch 事实。
    #[must_use]
    pub fn branches(&self) -> &[NullableWhenBranchFact] {
        &self.branches
    }
    /// 返回实际 Value 交付表达式的 Copy/Consume 事实；普通读取不在其中。
    #[must_use]
    pub fn extractions(&self) -> &[NullableWhenExtractionFact] {
        &self.extractions
    }
}
