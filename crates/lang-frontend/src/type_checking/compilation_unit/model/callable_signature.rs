//! Callable/调用结果描述符与其完整只读合同。
use super::*;
use crate::type_checking::BorrowReturnContract;

/// unit-wide 收集的 callable signature。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCallableSignature {
    target: UnitCallableTarget,
    name: String,
    name_span: Span,
    type_parameters: Vec<UnitSymbolId>,
    receiver: Option<UnitCallableReceiver>,
    parameters: Vec<UnitCallableParameter>,
    return_type: UnitTypeId,
    borrow_return: Option<BorrowReturnContract>,
    callable_type: UnitTypeId,
    visibility: DeclarationVisibility,
    has_body: bool,
}

impl UnitCallableSignature {
    /// 返回普通借用结果合同；None 表示 owned 结果。
    pub const fn borrow_return(&self) -> Option<BorrowReturnContract> {
        self.borrow_return
    }

    pub(crate) fn with_borrow_return(mut self, contract: Option<BorrowReturnContract>) -> Self {
        self.borrow_return = contract;
        self
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        target: UnitCallableTarget,
        name: String,
        name_span: Span,
        type_parameters: Vec<UnitSymbolId>,
        receiver: Option<UnitCallableReceiver>,
        parameters: Vec<UnitCallableParameter>,
        return_type: UnitTypeId,
        callable_type: UnitTypeId,
        visibility: DeclarationVisibility,
        has_body: bool,
    ) -> Self {
        Self {
            target,
            name,
            name_span,
            type_parameters,
            receiver,
            parameters,
            return_type,
            borrow_return: None,
            callable_type,
            visibility,
            has_body,
        }
    }

    /// 返回静态 target identity。
    #[must_use]
    pub const fn target(&self) -> UnitCallableTarget {
        self.target
    }

    /// 返回声明名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回名称范围。
    #[must_use]
    pub const fn name_span(&self) -> Span {
        self.name_span
    }

    /// 返回 callable 自身的类型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[UnitSymbolId] {
        &self.type_parameters
    }

    /// 返回 instance member 的 receiver 契约；顶层/companion callable 为 `None`。
    #[must_use]
    pub const fn receiver(&self) -> Option<UnitCallableReceiver> {
        self.receiver
    }

    /// 返回声明顺序参数。
    #[must_use]
    pub fn parameters(&self) -> &[UnitCallableParameter] {
        &self.parameters
    }

    /// 返回返回类型。
    #[must_use]
    pub const fn return_type(&self) -> UnitTypeId {
        self.return_type
    }

    /// 返回完整函数类型。
    #[must_use]
    pub const fn callable_type(&self) -> UnitTypeId {
        self.callable_type
    }

    /// 返回 callable 的规范化可见性。
    #[must_use]
    pub const fn visibility(&self) -> DeclarationVisibility {
        self.visibility
    }

    /// 返回 callable 是否具有源码 body。
    #[must_use]
    pub const fn has_body(&self) -> bool {
        self.has_body
    }
}
