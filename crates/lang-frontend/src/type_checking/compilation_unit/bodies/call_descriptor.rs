//! Callable/调用结果描述符与其完整只读合同。
use super::*;
use crate::type_checking::BorrowReturnContract;

/// 一个已唯一选择并完成首批 unit body 契约检查的 call。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCallDescriptor {
    pub(super) expression: UnitExpressionId,
    pub(super) instance: UnitCallableInstanceKey,
    pub(super) return_type: UnitTypeId,
    pub(super) result_source: crate::type_checking::CallableResultSource,
    pub(super) range_construction:
        Option<crate::type_checking::RangeConstructionDescriptor<UnitExpressionId, UnitTypeId>>,
    pub(super) receiver: Option<UnitCallReceiverDescriptor>,
    pub(super) arguments: Vec<UnitCallArgumentDescriptor>,
    pub(super) aborts: bool,
    pub(super) prints_line: bool,
}

impl UnitCallDescriptor {
    /// 返回 source-qualified 范围构造的实际操作数。
    pub fn range_construction(
        &self,
    ) -> Option<&crate::type_checking::RangeConstructionDescriptor<UnitExpressionId, UnitTypeId>>
    {
        self.range_construction.as_ref()
    }
    /// 返回 owned、既有存储借用或新 carrier 的封闭来源合同。
    pub const fn result_source(&self) -> crate::type_checking::CallableResultSource {
        self.result_source
    }

    /// 仅投影既有存储借用；None 也可能表示新 carrier 交付。
    pub const fn borrow_return(&self) -> Option<BorrowReturnContract> {
        self.result_source.borrow_return()
    }
    /// 返回带 source-unit 限定的 call expression identity。
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }

    /// 返回唯一静态 call target。
    #[must_use]
    pub const fn target(&self) -> UnitCallTarget {
        self.instance.target()
    }

    /// 返回 target 与完整类型实参组成的实例 identity。
    #[must_use]
    pub const fn instance(&self) -> &UnitCallableInstanceKey {
        &self.instance
    }

    /// 返回 call 的结果类型。
    #[must_use]
    pub const fn return_type(&self) -> UnitTypeId {
        self.return_type
    }

    /// 返回 member call 的隐藏 receiver；非 member call 为 `None`。
    #[must_use]
    pub const fn receiver(&self) -> Option<UnitCallReceiverDescriptor> {
        self.receiver
    }

    /// 返回源码实参顺序的参数映射。
    #[must_use]
    pub fn arguments(&self) -> &[UnitCallArgumentDescriptor] {
        &self.arguments
    }

    /// 返回 call target 是否具有编译器绑定的 abort effect。
    #[must_use]
    pub const fn aborts(&self) -> bool {
        self.aborts
    }

    /// 返回 call target 是否具有编译器绑定的 stdout 行输出 effect。
    #[must_use]
    pub const fn prints_line(&self) -> bool {
        self.prints_line
    }
}
