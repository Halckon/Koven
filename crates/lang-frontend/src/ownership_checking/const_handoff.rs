//! 常量专用 owned unit 的封闭借用；完整物化与短路能力不转换为 basic。

use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit},
    source::SourceMap,
    type_checking::{CompilationUnitTypes, ConstEnabledTypedUnit, TypeEnvironment},
};

use super::{CompilationUnitOwnership, ConstEnabledOwnedUnit, OwnedCompilationUnitViewError};

/// 已核对完整来源身份链的常量专用 owned compilation unit 只读借用。
///
/// 六项输入仍由调用方拥有，view 不能比任一借用存活更久；包括没有常量声明的专用产物。
/// 只有 [`const_owned_compilation_unit_view`] 能构造，不接受 basic 或 recovery capability。
pub struct ConstOwnedCompilationUnitView<'view, 'parsed: 'view> {
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    _environment: &'view TypeEnvironment,
    typed: &'view ConstEnabledTypedUnit,
    owned: &'view ConstEnabledOwnedUnit,
}

impl<'view, 'parsed: 'view> ConstOwnedCompilationUnitView<'view, 'parsed> {
    /// 返回被借用的源码集合。
    #[must_use]
    pub const fn sources(&self) -> &'view SourceMap {
        self.sources
    }

    /// 返回调用方的源码输入切片；不复制 AST。
    #[must_use]
    pub const fn inputs(&self) -> &'view [SourceUnitInput<'parsed>] {
        self.inputs
    }

    /// 返回已决定的名称身份和规范 index。
    #[must_use]
    pub const fn names(&self) -> &'view ValidatedCompilationUnitNames {
        self.names
    }

    /// 返回原类型事实，包括同轮常量 descriptor。
    #[must_use]
    pub const fn types(&self) -> &'view CompilationUnitTypes {
        self.typed.types()
    }

    /// 返回原只读所有权事实，不提供基础 validated capability。
    #[must_use]
    pub const fn ownership(&self) -> &'view CompilationUnitOwnership {
        self.owned.ownership()
    }

    /// 返回完整常量专用 capability，保留物化与短路计划的查询。
    #[must_use]
    pub const fn constant_ownership(&self) -> &'view ConstEnabledOwnedUnit {
        self.owned
    }
}

/// 以一次 index 重建核对 const unit 完整来源身份链，并封闭为只读借用。
///
/// 合法 clone、等价 inputs 与同 typed 重查 ownership 保留兼容性；不根据常量数量切换能力。
pub fn const_owned_compilation_unit_view<'view, 'parsed: 'view>(
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    environment: &'view TypeEnvironment,
    typed: &'view ConstEnabledTypedUnit,
    owned: &'view ConstEnabledOwnedUnit,
) -> Result<ConstOwnedCompilationUnitView<'view, 'parsed>, OwnedCompilationUnitViewError> {
    let rebuilt = index_compilation_unit(sources, inputs)
        .map_err(|_| OwnedCompilationUnitViewError::MismatchedSource)?;
    if &rebuilt != names.names().index()
        || !typed
            .types()
            .signatures()
            .is_compatible_with_index(&rebuilt, names, environment)
        || !owned.is_compatible_with(typed)
    {
        return Err(OwnedCompilationUnitViewError::MismatchedAnalysis);
    }
    Ok(ConstOwnedCompilationUnitView {
        sources,
        inputs,
        names,
        _environment: environment,
        typed,
        owned,
    })
}
