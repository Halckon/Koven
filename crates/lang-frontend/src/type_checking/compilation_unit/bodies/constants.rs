//! Constant-enabled unit facts are a separate capability from the base ownership input.
use super::{CompilationUnitTypes, UnitExpressionId, UnitTypeId};
use crate::{name_resolution::UnitSymbolId, source::Span, type_checking::ConstValue};
use std::collections::BTreeMap;

/// 已求值常量声明，所有 identity 均属于包含它的 compilation unit。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstantDescriptor {
    pub(super) symbol: UnitSymbolId,
    pub(super) declaration_span: Span,
    pub(super) ty: UnitTypeId,
    pub(super) value: ConstValue,
    pub(super) dependencies: Vec<UnitSymbolId>,
}
impl UnitConstantDescriptor {
    /// 返回 source-qualified 声明 symbol。
    pub const fn symbol(&self) -> UnitSymbolId {
        self.symbol
    }
    /// 返回常量声明名的位置。
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }
    /// 返回精确常量类型。
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }
    /// 返回编译器拥有的值。
    pub const fn value(&self) -> &ConstValue {
        &self.value
    }
    /// 返回按稳定 symbol 顺序排列的语法依赖，包括短路 RHS。
    pub fn dependencies(&self) -> &[UnitSymbolId] {
        &self.dependencies
    }
}

/// 已选择常量读取，包含初始化器依赖及运行时读取，与单文件 use shape 一致。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstantUseDescriptor {
    pub(super) expression: UnitExpressionId,
    pub(super) target: UnitSymbolId,
    pub(super) ty: UnitTypeId,
    pub(super) value: ConstValue,
}
impl UnitConstantUseDescriptor {
    /// 返回 source-qualified 读取表达式。
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }
    /// 返回已选择的常量 symbol。
    pub const fn target(&self) -> UnitSymbolId {
        self.target
    }
    /// 返回精确读取类型。
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }
    /// 返回该次读取的编译期值。
    pub const fn value(&self) -> &ConstValue {
        &self.value
    }
}

/// 同一次无错误 unit 分析原子发布的完整常量事实；不授予 ownership/native 能力。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstantFacts {
    pub(super) declarations: Vec<UnitConstantDescriptor>,
    pub(super) uses: Vec<UnitConstantUseDescriptor>,
}
impl UnitConstantFacts {
    /// 返回按稳定 symbol 顺序排列的常量声明。
    pub fn declarations(&self) -> &[UnitConstantDescriptor] {
        &self.declarations
    }
    /// 返回按 source-qualified expression 顺序排列的读取。
    pub fn uses(&self) -> &[UnitConstantUseDescriptor] {
        &self.uses
    }
}

/// 常量专用 typed capability；旧 ownership/native 入口不接受本类型。
///
/// ```compile_fail,E0308
/// use lang_frontend::{source::SourceMap, name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
///     type_checking::{ConstEnabledTypedUnit, TypeEnvironment}, ownership_checking::check_compilation_unit_ownership};
/// fn reject(sources: &SourceMap, inputs: &[SourceUnitInput<'_>], names: &ValidatedCompilationUnitNames,
///     environment: &TypeEnvironment, typed: &ConstEnabledTypedUnit) {
///     check_compilation_unit_ownership(sources, inputs, names, environment, typed);
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstEnabledTypedUnit(pub(super) CompilationUnitTypes);
impl ConstEnabledTypedUnit {
    /// 返回包含相同分析身份的 recovery typed 视图。
    pub const fn types(&self) -> &CompilationUnitTypes {
        &self.0
    }
    /// 返回在构造本 capability 时已验证完整的常量事实。
    pub fn constants(&self) -> &UnitConstantFacts {
        self.0
            .constants
            .as_ref()
            .expect("constant capability validates facts")
    }
    /// 解包回 recovery product；基础 validate 仍拒绝常量声明。
    pub fn into_types(self) -> CompilationUnitTypes {
        self.0
    }
}

pub(super) type ConstantInputs = BTreeMap<UnitSymbolId, (Span, Vec<UnitSymbolId>)>;
