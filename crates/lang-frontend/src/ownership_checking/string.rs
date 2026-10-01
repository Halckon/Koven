use crate::{
    ast::ExpressionId,
    type_checking::{
        StringOperationDescriptor, StringOperationKind, TypeId, UnitExpressionId,
        UnitStringOperationDescriptor, UnitTypeId,
    },
};

/// Phase 3 已批准的共享借用复制，结果承担独立 String owner 析构义务。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StringOwnershipEffect(StringOperationDescriptor);
impl StringOwnershipEffect {
    pub(crate) const fn new(descriptor: StringOperationDescriptor) -> Self {
        Self(descriptor)
    }
    /// 返回已通过所有权检查的调用。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.0.expression()
    }
    /// 返回共享借用且调用后仍可用的 receiver。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.0.receiver()
    }
    /// 返回源 builtin String 类型。
    #[must_use]
    pub const fn receiver_type(self) -> TypeId {
        self.0.receiver_type()
    }
    /// 返回独立结果 owner 的类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.0.result_type()
    }
    /// 返回批准的 intrinsic 身份。
    #[must_use]
    pub const fn kind(self) -> StringOperationKind {
        self.0.kind()
    }
}

/// Phase 3 已批准的共享借用复制，结果承担独立 String owner 析构义务。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitStringOwnershipEffect(UnitStringOperationDescriptor);
impl UnitStringOwnershipEffect {
    pub(crate) const fn new(descriptor: UnitStringOperationDescriptor) -> Self {
        Self(descriptor)
    }
    /// 返回已通过所有权检查的调用。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.0.expression()
    }
    /// 返回共享借用且调用后仍可用的 receiver。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.0.receiver()
    }
    /// 返回源 builtin String 类型。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.0.receiver_type()
    }
    /// 返回独立结果 owner 的类型。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.0.result_type()
    }
    /// 返回批准的 intrinsic 身份。
    #[must_use]
    pub const fn kind(self) -> StringOperationKind {
        self.0.kind()
    }
}
