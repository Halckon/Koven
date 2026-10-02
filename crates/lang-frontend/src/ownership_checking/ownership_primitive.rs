//! 已检查 owned mutable root 原子 commit；静态 descriptor 本身不授予此能力。
use super::{OwnershipPlace, UnitOwnershipPlace};
use crate::type_checking::{OwnershipPrimitiveDescriptor, UnitOwnershipPrimitiveDescriptor};

/// replace 的第二实参已按普通 Value 参数完成的交付。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipPrimitiveValueTransfer {
    /// Copyable 输入，保留源值。
    Copy,
    /// MoveOnly 完整 place 输入，消费源 owner。
    Move,
    /// 本次求值结果直接交付；MoveOnly 时转移 temporary owner。
    Temporary,
}

/// 正常求值前缀后，在既有 exclusive loans 保护下提交的原子置换。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnershipPrimitiveOwnershipPlan {
    pub(crate) descriptor: OwnershipPrimitiveDescriptor,
    pub(crate) places: Vec<OwnershipPlace>,
    pub(crate) new_value_transfer: Option<OwnershipPrimitiveValueTransfer>,
}

impl OwnershipPrimitiveOwnershipPlan {
    /// 与本次求值绑定的 compiler intrinsic 身份。
    #[must_use]
    pub const fn descriptor(&self) -> &OwnershipPrimitiveDescriptor {
        &self.descriptor
    }
    /// 源码顺序的 owned mutable 完整 roots；replace 一项，swap 两项。
    #[must_use]
    pub fn places(&self) -> &[OwnershipPlace] {
        &self.places
    }
    /// replace 的新值交付方式；swap 不产生新值。
    #[must_use]
    pub const fn new_value_transfer(&self) -> Option<OwnershipPrimitiveValueTransfer> {
        self.new_value_transfer
    }
}

/// source-qualified 的正常 root commit，不能由外部伪造。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnershipPrimitiveOwnershipPlan {
    pub(crate) descriptor: UnitOwnershipPrimitiveDescriptor,
    pub(crate) places: Vec<UnitOwnershipPlace>,
    pub(crate) new_value_transfer: Option<OwnershipPrimitiveValueTransfer>,
}

impl UnitOwnershipPrimitiveOwnershipPlan {
    /// 与本次求值绑定的 compiler intrinsic 身份。
    #[must_use]
    pub const fn descriptor(&self) -> &UnitOwnershipPrimitiveDescriptor {
        &self.descriptor
    }
    /// 源码顺序的 owned mutable 完整 roots；replace 一项，swap 两项。
    #[must_use]
    pub fn places(&self) -> &[UnitOwnershipPlace] {
        &self.places
    }
    /// replace 的新值交付方式；swap 不产生新值。
    #[must_use]
    pub const fn new_value_transfer(&self) -> Option<OwnershipPrimitiveValueTransfer> {
        self.new_value_transfer
    }
}
