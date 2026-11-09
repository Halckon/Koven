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

/// String.clone receiver 在 planner 正常完成后的独立结束观察，不是动态执行 trace。
/// 仅含本 intrinsic 的 Shared loan；不表示完整 loan-end 流或与 drops 的跨表总序。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitStringCloneLoanEndFact {
    loan: super::UnitLoanFact,
}
impl UnitStringCloneLoanEndFact {
    pub(crate) const fn new(loan: super::UnitLoanFact) -> Self {
        Self { loan }
    }

    /// 返回本次正常完成的原 source-qualified receiver loan。
    #[must_use]
    pub const fn loan(&self) -> &super::UnitLoanFact {
        &self.loan
    }

    /// 返回实际规划完成的 clone 调用点；不代表其 receiver owner 已析构。
    #[must_use]
    pub const fn point(&self) -> super::UnitDropPoint {
        super::UnitDropPoint::CallReturn(self.loan.call())
    }
}
