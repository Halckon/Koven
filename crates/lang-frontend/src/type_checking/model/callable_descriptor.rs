//! Callable/调用结果描述符与其完整只读合同。
use super::*;
use crate::type_checking::BorrowReturnContract;

/// 已规范化的顶层或实例 member callable 签名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableDescriptor {
    pub(crate) symbol: SymbolId,
    pub(crate) owner: Option<NominalId>,
    pub(crate) extension_receiver_symbol: Option<SymbolId>,
    pub(crate) receiver: Option<CallableReceiverDescriptor>,
    pub(crate) type_parameters: Vec<SymbolId>,
    pub(crate) parameter_symbols: Vec<Option<SymbolId>>,
    pub(crate) parameters: Vec<FunctionParameterType>,
    pub(crate) return_type: TypeId,
    pub(crate) result_source: crate::type_checking::CallableResultSource,
    pub(crate) range_extension: Option<
        crate::type_checking::RangeExtensionBinding<crate::type_checking::CallableTarget, TypeId>,
    >,
}

impl CallableDescriptor {
    /// 可信扩展接收者的实际合成 Borrow 参数，与 canonical 声明同轮绑定。
    pub(crate) const fn extension_receiver_symbol(&self) -> Option<SymbolId> {
        self.extension_receiver_symbol
    }
    /// 受限扩展签名的可信绑定；不代表 ownership 或 lowering 已接通。
    pub const fn range_extension(
        &self,
    ) -> Option<
        crate::type_checking::RangeExtensionBinding<crate::type_checking::CallableTarget, TypeId>,
    > {
        self.range_extension
    }
    /// 返回 owned、既有存储借用或新 carrier 的封闭来源合同。
    pub const fn result_source(&self) -> crate::type_checking::CallableResultSource {
        self.result_source
    }

    /// 仅投影既有存储借用；None 也可能表示新 carrier 交付。
    pub const fn borrow_return(&self) -> Option<BorrowReturnContract> {
        self.result_source.borrow_return()
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
    /// 返回实例 member 或受限扩展的 receiver 契约；普通顶层 callable 为 `None`。
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
