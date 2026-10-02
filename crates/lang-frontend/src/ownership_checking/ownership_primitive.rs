//! 已检查 root / 一级字段原子 commit；二者能力分离，静态 descriptor 不授予提交权限。
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

/// 已独占的 owned local 普通 class 一级字段原子置换；不消费 parent owner。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldReplaceOwnershipPlan {
    pub(crate) descriptor: OwnershipPrimitiveDescriptor,
    pub(crate) place: OwnershipPlace,
    pub(crate) owner_type: crate::type_checking::TypeId,
    pub(crate) new_value_transfer: OwnershipPrimitiveValueTransfer,
}

impl FieldReplaceOwnershipPlan {
    /// 已检查的 replace intrinsic 身份。
    #[must_use]
    pub const fn descriptor(&self) -> &OwnershipPrimitiveDescriptor {
        &self.descriptor
    }
    /// 恰好一个字段投影，不包含 index。
    #[must_use]
    pub const fn place(&self) -> &OwnershipPlace {
        &self.place
    }
    /// 保持 owning、完全初始化的普通 class receiver 类型。
    #[must_use]
    pub const fn owner_type(&self) -> crate::type_checking::TypeId {
        self.owner_type
    }
    /// 新字段值已完成的 Value 交付。
    #[must_use]
    pub const fn new_value_transfer(&self) -> OwnershipPrimitiveValueTransfer {
        self.new_value_transfer
    }
}

/// source-qualified 一级字段置换能力，与 root 原语计划严格分离。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitFieldReplaceOwnershipPlan {
    pub(crate) descriptor: UnitOwnershipPrimitiveDescriptor,
    pub(crate) place: UnitOwnershipPlace,
    pub(crate) owner_type: crate::type_checking::UnitTypeId,
    pub(crate) new_value_transfer: OwnershipPrimitiveValueTransfer,
}

impl UnitFieldReplaceOwnershipPlan {
    /// 已检查的 replace intrinsic 身份。
    #[must_use]
    pub const fn descriptor(&self) -> &UnitOwnershipPrimitiveDescriptor {
        &self.descriptor
    }
    /// 恰好一个字段投影，不包含 index。
    #[must_use]
    pub const fn place(&self) -> &UnitOwnershipPlace {
        &self.place
    }
    /// 保持 owning、完全初始化的普通 class receiver 类型。
    #[must_use]
    pub const fn owner_type(&self) -> crate::type_checking::UnitTypeId {
        self.owner_type
    }
    /// 新字段值已完成的 Value 交付。
    #[must_use]
    pub const fn new_value_transfer(&self) -> OwnershipPrimitiveValueTransfer {
        self.new_value_transfer
    }
}

/// 置换能力只覆盖 lexical owned local；声明收集表也包含 globals，不能单凭表成员资格授权。
pub(super) fn is_local_variable(
    names: &crate::name_resolution::NameResolution,
    id: crate::name_resolution::SymbolId,
) -> bool {
    use crate::name_resolution::{ScopeKind, SymbolKind};
    let Some(symbol) = names.symbols().get(id.index()) else {
        return false;
    };
    symbol.kind() == SymbolKind::Variable
        && names
            .scopes()
            .get(symbol.scope().index())
            .is_some_and(|scope| {
                matches!(
                    scope.kind(),
                    ScopeKind::Function
                        | ScopeKind::Lambda
                        | ScopeKind::Block
                        | ScopeKind::ControlBody
                        | ScopeKind::Loop
                )
            })
}
