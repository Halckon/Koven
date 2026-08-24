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
    arguments: Vec<CallArgumentDescriptor>,
    aborts: bool,
}

impl CallDescriptor {
    pub(crate) fn new(
        expression: ExpressionId,
        target: CallableTarget,
        type_arguments: Vec<TypeId>,
        return_type: TypeId,
        arguments: Vec<CallArgumentDescriptor>,
        aborts: bool,
    ) -> Self {
        Self {
            expression,
            instance: CallableInstanceKey::new(target, type_arguments),
            return_type,
            arguments,
            aborts,
        }
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
}
