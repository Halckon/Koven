//! 单文件 Phase 2 常量事实；不证明 Phase 3 物化、所有权或 native 能力。

use super::{ConstValue, ExpressionCategory, TypeId};
use crate::{ast::ExpressionId, name_resolution::SymbolId, source::Span};

/// 一个已完成类型检查与求值的声明。
#[derive(Clone, Debug)]
pub struct ConstantDescriptor {
    pub(crate) symbol: SymbolId,
    pub(crate) declaration_span: Span,
    pub(crate) ty: TypeId,
    pub(crate) value: ConstValue,
    pub(crate) dependencies: Vec<SymbolId>,
}

impl ConstantDescriptor {
    /// 声明的单文件身份。
    #[must_use]
    pub fn symbol(&self) -> SymbolId {
        self.symbol
    }
    /// 声明名称的位置。
    #[must_use]
    pub fn declaration_span(&self) -> Span {
        self.declaration_span
    }
    /// 与 typed symbol 相同的精确类型。
    #[must_use]
    pub fn ty(&self) -> TypeId {
        self.ty
    }
    /// 编译器拥有的值，不是运行时 owner。
    #[must_use]
    pub fn value(&self) -> &ConstValue {
        &self.value
    }
    /// 包含短路 RHS 的语法依赖，按 symbol 顺序排列。
    #[must_use]
    pub fn dependencies(&self) -> &[SymbolId] {
        &self.dependencies
    }
}

/// bare/qualified 常量读取，包含 initializer 内与运行时位置的读取。
#[derive(Clone, Debug)]
pub struct ConstantUseDescriptor {
    pub(crate) expression: ExpressionId,
    pub(crate) target: SymbolId,
    pub(crate) ty: TypeId,
    pub(crate) value: ConstValue,
}

impl ConstantUseDescriptor {
    /// 读取表达式的身份；group 本身不重复创建读取。
    #[must_use]
    pub fn expression(&self) -> ExpressionId {
        self.expression
    }
    /// 与声明 descriptor 对应的常量身份。
    #[must_use]
    pub fn target(&self) -> SymbolId {
        self.target
    }
    /// 精确表达式类型。
    #[must_use]
    pub fn ty(&self) -> TypeId {
        self.ty
    }
    /// 后续阶段可用于重新物化的编译器值。
    #[must_use]
    pub fn value(&self) -> &ConstValue {
        &self.value
    }
    /// 常量读取从不创建字段 Place。
    #[must_use]
    pub fn category(&self) -> ExpressionCategory {
        ExpressionCategory::Temporary
    }
}

/// 仅由 checker 发布的完整常量集合，随所属 TypedFile 绑定分析身份。
/// 不替代 ownership/native 验证，也不包含跨文件事实。
#[derive(Clone, Debug)]
pub struct ValidatedConstants {
    pub(crate) analysis_owner: std::sync::Arc<()>,
    pub(crate) declarations: Vec<ConstantDescriptor>,
    pub(crate) uses: Vec<ConstantUseDescriptor>,
}

impl ValidatedConstants {
    /// 验证该集合来自当前 typed 分析；仅源码相同不足以匹配。
    #[must_use]
    pub fn matches(&self, typed: &super::TypedFile) -> bool {
        std::sync::Arc::ptr_eq(&self.analysis_owner, typed.analysis_owner())
    }

    /// 源文件中已求值的常量，按 symbol 顺序排列。
    #[must_use]
    pub fn declarations(&self) -> &[ConstantDescriptor] {
        &self.declarations
    }
    /// 已类型检查的常量读取，按 expression 顺序排列。
    #[must_use]
    pub fn uses(&self) -> &[ConstantUseDescriptor] {
        &self.uses
    }
    /// 以当前文件的表达式身份查询读取事实。
    #[must_use]
    pub fn use_at(&self, expression: ExpressionId) -> Option<&ConstantUseDescriptor> {
        self.uses
            .binary_search_by_key(&expression.index(), |usage| usage.expression.index())
            .ok()
            .map(|index| &self.uses[index])
    }
}
