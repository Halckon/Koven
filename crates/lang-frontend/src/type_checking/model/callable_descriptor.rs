//! Callable/调用结果描述符与其完整只读合同。
use super::*;
use crate::type_checking::BorrowReturnContract;

/// 已规范化的顶层或实例 member callable 签名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableDescriptor {
    pub(crate) symbol: SymbolId,
    pub(crate) owner: Option<NominalId>,
    pub(crate) receiver: Option<CallableReceiverDescriptor>,
    pub(crate) type_parameters: Vec<SymbolId>,
    pub(crate) parameter_symbols: Vec<Option<SymbolId>>,
    pub(crate) parameters: Vec<FunctionParameterType>,
    pub(crate) return_type: TypeId,
    pub(crate) borrow_return: Option<BorrowReturnContract>,
}

impl CallableDescriptor {
    /// 返回普通借用结果的来源合同；None 表示 owned 结果。
    pub const fn borrow_return(&self) -> Option<BorrowReturnContract> {
        self.borrow_return
    }
    /// 返回函数声明 symbol。
    #[must_use]
    pub const fn symbol(&self) -> SymbolId {
        self.symbol
    }
    /// 返回实例 member owner；顶层函数为 `None`。
    #[must_use]
    pub const fn owner(&self) -> Option<NominalId> {
        self.owner
    }
    /// 返回 instance member 的隐藏 receiver 契约；顶层 callable 为 `None`。
    #[must_use]
    pub const fn receiver(&self) -> Option<CallableReceiverDescriptor> {
        self.receiver
    }
    /// 返回 callable 自身的源码顺序类型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[SymbolId] {
        &self.type_parameters
    }
    /// 返回与参数顺序对齐的稳定名称 symbol；恢复参数为 `None`。
    #[must_use]
    pub fn parameter_symbols(&self) -> &[Option<SymbolId>] {
        &self.parameter_symbols
    }
    /// 返回包含参数模式的规范化参数。
    #[must_use]
    pub fn parameters(&self) -> &[FunctionParameterType] {
        &self.parameters
    }
    /// 返回规范化返回类型。
    #[must_use]
    pub const fn return_type(&self) -> TypeId {
        self.return_type
    }
}
