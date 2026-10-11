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
    result_source: crate::type_checking::CallableResultSource,
    callable_type: UnitTypeId,
    visibility: DeclarationVisibility,
    has_body: bool,
    extension_receiver_symbol: Option<UnitSymbolId>,
    range_extension:
        Option<crate::type_checking::RangeExtensionBinding<UnitCallableTarget, UnitTypeId>>,
}

impl UnitCallableSignature {
    /// 同一 SourceUnit 中锚定到可信扩展 receiver 的合成参数身份。
    pub const fn extension_receiver_symbol(&self) -> Option<UnitSymbolId> {
        self.extension_receiver_symbol
    }
    pub(crate) fn with_extension_receiver_symbol(mut self, symbol: Option<UnitSymbolId>) -> Self {
        self.extension_receiver_symbol = symbol;
        self
    }
    /// 受限扩展签名绑定；实际来源由所有权阶段检查，后端仍有能力门。
    pub const fn range_extension(
        &self,
    ) -> Option<crate::type_checking::RangeExtensionBinding<UnitCallableTarget, UnitTypeId>> {
        self.range_extension
    }
    pub(crate) fn with_range_extension(
        mut self,
        binding: Option<
            crate::type_checking::RangeExtensionBinding<UnitCallableTarget, UnitTypeId>,
        >,
    ) -> Self {
        self.range_extension = binding;
        self
    }
    /// 返回 owned、既有存储借用或新 carrier 的封闭来源合同。
    pub const fn result_source(&self) -> crate::type_checking::CallableResultSource {
        self.result_source
    }

    /// 仅投影既有存储借用；None 也可能表示新 carrier 交付。
    pub const fn borrow_return(&self) -> Option<BorrowReturnContract> {
        self.result_source.borrow_return()
    }

    pub(crate) fn with_result_source(
        mut self,
        contract: crate::type_checking::CallableResultSource,
    ) -> Self {
        self.result_source = contract;
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
            result_source: crate::type_checking::CallableResultSource::Owned,
            callable_type,
            visibility,
            has_body,
            range_extension: None,
            extension_receiver_symbol: None,
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

    /// 返回实例 member 或受限扩展的 receiver 契约；普通顶层/companion 为 `None`。
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
