//! `!!` 的已检查 non-null 转移与封闭 null Abort 边。
use super::OwnershipPlace;
use crate::type_checking::{AssertionFailureEffect, NonNullAssertionDescriptor};

/// 只发生在 assertion 成功证明非空的边上的 inner 交付。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonNullAssertionTransferKind {
    /// 复制 inner，保留 nullable 来源。
    Copy,
    /// 整体消费 nullable owner，将唯一 inner 析构义务交付 assertion 结果。
    Consume,
}

/// 仅在整个所有权分析无诊断时发布；失败边不包含 take、正常后继或 unwind cleanup。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonNullAssertionOwnershipPlan {
    pub(crate) descriptor: NonNullAssertionDescriptor,
    pub(crate) source_place: Option<OwnershipPlace>,
    pub(crate) non_null_transfer: NonNullAssertionTransferKind,
}

impl NonNullAssertionOwnershipPlan {
    /// 原 typed descriptor；operand 与 assertion 身份绑定唯一一次求值和结果。
    #[must_use]
    pub fn descriptor(&self) -> &NonNullAssertionDescriptor {
        &self.descriptor
    }

    /// 来源的 root/field/element identity；临时来源没有可重复读取的 place。
    #[must_use]
    pub fn source_place(&self) -> Option<&OwnershipPlace> {
        self.source_place.as_ref()
    }

    /// 仅在 non-null proof 成立后执行；结果后续清理由通常的 drop facts 表达。
    #[must_use]
    pub fn non_null_transfer(&self) -> NonNullAssertionTransferKind {
        self.non_null_transfer
    }

    /// null 边直接 Abort，不执行上面的 transfer，也不展开析构。
    #[must_use]
    pub fn null_effect(&self) -> AssertionFailureEffect {
        self.descriptor.failure_effect()
    }
}
