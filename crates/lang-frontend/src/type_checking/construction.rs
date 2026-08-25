use crate::{
    ast::ExpressionId,
    name_resolution::{EnumCaseId, SymbolId},
};

use super::{ExpressionCategory, NominalId, ParameterMode, TypeId};

/// 一个成功 construction 的唯一静态目标。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructionTarget {
    /// 源码普通 class 或 value class。
    Nominal(NominalId),
    /// 源码 enum case。
    EnumCase(EnumCaseId),
    /// 编译器绑定的 intrinsic `Box`。
    IntrinsicBox,
}

/// construction target 与完整类型实参组成的稳定实例 identity。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionInstanceKey {
    target: ConstructionTarget,
    type_arguments: Vec<TypeId>,
}

impl ConstructionInstanceKey {
    pub(crate) fn new(target: ConstructionTarget, type_arguments: Vec<TypeId>) -> Self {
        Self {
            target,
            type_arguments,
        }
    }

    #[must_use]
    /// 返回源码中被构造的静态目标。
    pub const fn target(&self) -> ConstructionTarget {
        self.target
    }

    #[must_use]
    /// 返回按声明顺序排列的完整类型实参。
    pub fn type_arguments(&self) -> &[TypeId] {
        &self.type_arguments
    }
}

/// 一个声明顺序的 construction 参数及其源码 operand 映射。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionArgumentDescriptor {
    parameter_index: usize,
    parameter_symbol: Option<SymbolId>,
    parameter_name: String,
    parameter_type: TypeId,
    mode: ParameterMode,
    argument: ExpressionId,
    evaluation_index: usize,
    category: ExpressionCategory,
}

impl ConstructionArgumentDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        parameter_index: usize,
        parameter_symbol: Option<SymbolId>,
        parameter_name: String,
        parameter_type: TypeId,
        argument: ExpressionId,
        evaluation_index: usize,
        category: ExpressionCategory,
    ) -> Self {
        Self {
            parameter_index,
            parameter_symbol,
            parameter_name,
            parameter_type,
            mode: ParameterMode::Value,
            argument,
            evaluation_index,
            category,
        }
    }

    #[must_use]
    /// 返回参数在构造签名中的声明序号。
    pub const fn parameter_index(&self) -> usize {
        self.parameter_index
    }
    #[must_use]
    /// 返回源码参数符号；intrinsic 参数没有源码符号。
    pub const fn parameter_symbol(&self) -> Option<SymbolId> {
        self.parameter_symbol
    }
    #[must_use]
    /// 返回参数名。
    pub fn parameter_name(&self) -> &str {
        &self.parameter_name
    }
    #[must_use]
    /// 返回完成实例化后的参数类型。
    pub const fn parameter_type(&self) -> TypeId {
        self.parameter_type
    }
    #[must_use]
    /// 返回参数传递模式；v0.29 construction 恒为 `Value`。
    pub const fn mode(&self) -> ParameterMode {
        self.mode
    }
    #[must_use]
    /// 返回映射到该参数的源码 operand。
    pub const fn argument(&self) -> ExpressionId {
        self.argument
    }
    #[must_use]
    /// 返回 operand 的源码求值序号。
    pub const fn evaluation_index(&self) -> usize {
        self.evaluation_index
    }
    #[must_use]
    /// 返回 operand 完成类型检查后的表达式类别。
    pub const fn category(&self) -> ExpressionCategory {
        self.category
    }
}

/// Phase 2 已完成选择、实例化和 operand 检查的 construction。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionDescriptor {
    expression: ExpressionId,
    instance: ConstructionInstanceKey,
    result_type: TypeId,
    arguments: Vec<ConstructionArgumentDescriptor>,
}

impl ConstructionDescriptor {
    pub(crate) fn new(
        expression: ExpressionId,
        target: ConstructionTarget,
        type_arguments: Vec<TypeId>,
        result_type: TypeId,
        arguments: Vec<ConstructionArgumentDescriptor>,
    ) -> Self {
        Self {
            expression,
            instance: ConstructionInstanceKey::new(target, type_arguments),
            result_type,
            arguments,
        }
    }

    #[must_use]
    /// 返回产生该 construction 的表达式。
    pub const fn expression(&self) -> ExpressionId {
        self.expression
    }
    #[must_use]
    /// 返回源码中被构造的静态目标。
    pub const fn target(&self) -> ConstructionTarget {
        self.instance.target()
    }
    #[must_use]
    /// 返回包含完整类型实参的 construction 实例 identity。
    pub const fn instance(&self) -> &ConstructionInstanceKey {
        &self.instance
    }
    #[must_use]
    /// 返回 construction 的结果类型。
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }
    #[must_use]
    /// 返回按参数声明顺序排列的 operand 映射。
    pub fn arguments(&self) -> &[ConstructionArgumentDescriptor] {
        &self.arguments
    }
}
