use crate::{
    ast::ExpressionId,
    name_resolution::{ExternalSymbolId, SymbolId},
};

use super::ParameterMode;
use super::model::TypeId;

/// Phase 2 对表达式操作数建立的类型层面类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpressionCategory {
    /// 具有稳定存储位置的表达式；可变性与当前所有权状态由 Phase 3 判断。
    Place,
    /// 本次求值产生的临时值。
    Temporary,
}

/// member call 的源码 receiver 来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallReceiverOrigin {
    /// `receiver.member(...)` 中显式求值一次的 receiver expression。
    Expression(ExpressionId),
    /// 裸 member call 复用当前 callable 的唯一 `this` binding。
    ImplicitThis(super::NominalId),
}

/// 成功 member call 的实例化 receiver 契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallReceiverDescriptor {
    pub(crate) origin: CallReceiverOrigin,
    pub(crate) mode: ParameterMode,
    pub(crate) category: ExpressionCategory,
    pub(crate) ty: TypeId,
}

impl CallReceiverDescriptor {
    /// 返回显式 expression 或隐式 `this` 来源。
    #[must_use]
    pub const fn origin(self) -> CallReceiverOrigin {
        self.origin
    }

    /// 返回已选择 callable 的 receiver mode。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回 receiver 的 place/temporary 类别。
    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    /// 返回 owner 类型实参替换后的 receiver 类型。
    #[must_use]
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// 一个成功 call 的静态目标。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableTarget {
    /// 当前源码文件内的具名 callable。
    Source(SymbolId),
    /// 显式名称/类型环境中的预声明 callable。
    External(ExternalSymbolId),
    /// 只由函数类型描述的普通函数值。
    FunctionValue,
    /// `value class` 自动结构分量；payload 是对应字段 symbol。
    StructuralComponent(SymbolId),
}

/// 一个已实例化 callable 的稳定类型层 identity。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableInstanceKey {
    target: CallableTarget,
    type_arguments: Vec<TypeId>,
}

impl CallableInstanceKey {
    pub(crate) fn new(target: CallableTarget, type_arguments: Vec<TypeId>) -> Self {
        Self {
            target,
            type_arguments,
        }
    }

    /// 返回唯一静态 callable 目标。
    #[must_use]
    pub const fn target(&self) -> CallableTarget {
        self.target
    }

    /// 返回 owner 参数在前、callable 参数在后的完整实例实参。
    #[must_use]
    pub fn type_arguments(&self) -> &[TypeId] {
        &self.type_arguments
    }
}

/// 一个源码实参到 callable 参数的稳定映射。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallArgumentDescriptor {
    argument_index: usize,
    parameter_index: usize,
    category: ExpressionCategory,
    mode: ParameterMode,
    parameter_type: TypeId,
    cross_thread: bool,
}

impl CallArgumentDescriptor {
    pub(crate) const fn new(
        argument_index: usize,
        parameter_index: usize,
        category: ExpressionCategory,
        mode: ParameterMode,
        parameter_type: TypeId,
        cross_thread: bool,
    ) -> Self {
        Self {
            argument_index,
            parameter_index,
            category,
            mode,
            parameter_type,
            cross_thread,
        }
    }

    /// 返回源码顺序的实参下标。
    #[must_use]
    pub const fn argument_index(self) -> usize {
        self.argument_index
    }

    /// 返回声明顺序的参数下标。
    #[must_use]
    pub const fn parameter_index(self) -> usize {
        self.parameter_index
    }

    /// 返回该 operand 的类型层面类别。
    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    /// 返回已选择参数的传递模式。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回 callable 实例化后的参数类型。
    #[must_use]
    pub const fn parameter_type(self) -> TypeId {
        self.parameter_type
    }

    /// 返回该参数是否由 compiler-bound effect 跨线程交付。
    #[must_use]
    pub const fn crosses_thread(self) -> bool {
        self.cross_thread
    }
}

/// 一个已唯一选择并完成 Phase 2 契约检查的 call。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallDescriptor {
    expression: ExpressionId,
    instance: CallableInstanceKey,
    return_type: TypeId,
    result_source: super::CallableResultSource,
    range_construction: Option<super::RangeConstructionDescriptor<ExpressionId, TypeId>>,
    receiver: Option<CallReceiverDescriptor>,
    arguments: Vec<CallArgumentDescriptor>,
    aborts: bool,
    prints_line: bool,
}

impl CallDescriptor {
    pub(crate) fn clear_range_construction(&mut self) {
        self.range_construction = None;
    }
    pub(crate) fn with_range_construction(
        mut self,
        descriptor: super::RangeConstructionDescriptor<ExpressionId, TypeId>,
    ) -> Self {
        self.range_construction = Some(descriptor);
        self
    }
    /// 返回可复用范围构造的实际操作数；普通 callable 没有该事实。
    pub fn range_construction(
        &self,
    ) -> Option<&super::RangeConstructionDescriptor<ExpressionId, TypeId>> {
        self.range_construction.as_ref()
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        expression: ExpressionId,
        target: CallableTarget,
        type_arguments: Vec<TypeId>,
        return_type: TypeId,
        receiver: Option<CallReceiverDescriptor>,
        arguments: Vec<CallArgumentDescriptor>,
        aborts: bool,
        prints_line: bool,
    ) -> Self {
        Self {
            expression,
            instance: CallableInstanceKey::new(target, type_arguments),
            return_type,
            result_source: crate::type_checking::CallableResultSource::Owned,
            range_construction: None,
            receiver,
            arguments,
            aborts,
            prints_line,
        }
    }

    /// 返回 call expression identity。
    pub(crate) fn with_result_source(mut self, contract: super::CallableResultSource) -> Self {
        self.result_source = contract;
        self
    }

    /// 返回 owned、既有存储借用或新 carrier 的封闭来源合同。
    pub const fn result_source(&self) -> crate::type_checking::CallableResultSource {
        self.result_source
    }

    /// 仅投影既有存储借用；None 也可能表示新 carrier 交付。
    pub const fn borrow_return(&self) -> Option<super::BorrowReturnContract> {
        self.result_source.borrow_return()
    }

    /// 返回 call expression identity。
    #[must_use]
    pub const fn expression(&self) -> ExpressionId {
        self.expression
    }

    /// 返回唯一静态目标。
    #[must_use]
    pub const fn target(&self) -> CallableTarget {
        self.instance.target()
    }

    /// 返回静态目标与完整类型实参组成的实例 key。
    #[must_use]
    pub const fn instance(&self) -> &CallableInstanceKey {
        &self.instance
    }

    /// 返回实例化后的返回类型。
    #[must_use]
    pub const fn return_type(&self) -> TypeId {
        self.return_type
    }

    /// 返回 member call 的隐藏 receiver；非 member call 为 `None`。
    #[must_use]
    pub const fn receiver(&self) -> Option<CallReceiverDescriptor> {
        self.receiver
    }

    /// 返回源码实参顺序的参数映射。
    #[must_use]
    pub fn arguments(&self) -> &[CallArgumentDescriptor] {
        &self.arguments
    }

    /// 返回该静态 call target 是否具有编译器绑定的不可捕获 abort effect。
    #[must_use]
    pub const fn aborts(&self) -> bool {
        self.aborts
    }

    /// 返回该静态 call target 是否具有编译器绑定的 stdout 行输出 effect。
    #[must_use]
    pub const fn prints_line(&self) -> bool {
        self.prints_line
    }
}
