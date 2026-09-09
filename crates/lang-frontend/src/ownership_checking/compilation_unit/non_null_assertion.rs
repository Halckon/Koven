//! source-qualified assertion 的已检查成功转移与 null Abort。
use super::UnitOwnershipPlace;
use crate::{
    ownership_checking::NonNullAssertionTransferKind,
    type_checking::{AssertionFailureEffect, UnitNonNullAssertionDescriptor},
};

/// 仅在整个所有权分析无诊断时发布；失败边不包含 take、正常后继或 unwind cleanup。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNonNullAssertionOwnershipPlan {
    pub(crate) descriptor: UnitNonNullAssertionDescriptor,
    pub(crate) source_place: Option<UnitOwnershipPlace>,
    pub(crate) non_null_transfer: NonNullAssertionTransferKind,
}

impl UnitNonNullAssertionOwnershipPlan {
    /// 原 typed descriptor；operand 与 assertion 身份绑定唯一一次求值和结果。
    #[must_use]
    pub fn descriptor(&self) -> &UnitNonNullAssertionDescriptor {
        &self.descriptor
    }

    /// 来源的 root/field/element identity；临时来源没有可重复读取的 place。
    #[must_use]
    pub fn source_place(&self) -> Option<&UnitOwnershipPlace> {
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
