//! Independent constant ownership capability; never converts to the base native input.
use std::sync::Arc;

use super::{CompilationUnitOwnership, OwnershipCheckingError, analysis};
use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::ConstantMaterializationKind,
    source::SourceMap,
    type_checking::{
        ConstEnabledTypedUnit, TypeEnvironment, UnitConstantUseDescriptor, UnitExpressionId,
    },
};

/// One runtime use of the exact Phase 2 constant value; cleanup is in the same ownership product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitConstantMaterializationPlan {
    pub(super) descriptor: UnitConstantUseDescriptor,
    pub(super) kind: ConstantMaterializationKind,
}

impl UnitConstantMaterializationPlan {
    /// 返回该次读取的 source-qualified identity、target、精确类型与编译期值。
    #[must_use]
    pub const fn descriptor(&self) -> &UnitConstantUseDescriptor {
        &self.descriptor
    }

    /// 返回标量 inline 或独立 String temporary 的物化类别。
    #[must_use]
    pub const fn kind(&self) -> ConstantMaterializationKind {
        self.kind
    }
}

/// 专用常量 ownership recovery；无完整物化/drop 事实时不能产生 validated capability。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitConstantOwnership(CompilationUnitOwnership);

impl CompilationUnitConstantOwnership {
    /// 返回普通 ownership 事实；克隆该 recovery 也不能通过基础 validate。
    #[must_use]
    pub const fn ownership(&self) -> &CompilationUnitOwnership {
        &self.0
    }

    /// 错误或 deferred 时为 None；空切片表示没有运行时常量读取。
    #[must_use]
    pub fn materializations(&self) -> Option<&[UnitConstantMaterializationPlan]> {
        self.0.constant_materializations.as_deref()
    }

    /// 同一轮 typed 分析的克隆保留身份；相同源码的重新分析不保留身份。
    #[must_use]
    pub fn is_compatible_with(&self, typed: &ConstEnabledTypedUnit) -> bool {
        Arc::ptr_eq(
            &self.0.provenance.typed_analysis_owner,
            typed.types().analysis_owner(),
        )
    }

    /// 仅在主分析已原子发布完整物化事实时验证；不会取得基础 native capability。
    pub fn validate(self) -> Result<ConstEnabledOwnedUnit, Box<Self>> {
        if self.0.constant_materializations.is_some() {
            Ok(ConstEnabledOwnedUnit(self))
        } else {
            Err(Box::new(self))
        }
    }
}

/// 常量专用无错误、无 deferred 的 unit ownership capability。
///
/// ```compile_fail,E0308
/// use lang_frontend::ownership_checking::{ConstEnabledOwnedUnit, ValidatedCompilationUnitOwnership};
/// fn reject(owned: &ConstEnabledOwnedUnit) {
///     let base: &ValidatedCompilationUnitOwnership = owned;
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstEnabledOwnedUnit(CompilationUnitConstantOwnership);

impl ConstEnabledOwnedUnit {
    /// 返回同一分析的 loan、delivery、capture 和 drop 事实。
    #[must_use]
    pub const fn ownership(&self) -> &CompilationUnitOwnership {
        self.0.ownership()
    }

    /// 按 source-qualified expression 稳定排序；初始化器和未访问读取没有计划。
    #[must_use]
    pub fn materializations(&self) -> &[UnitConstantMaterializationPlan] {
        self.0
            .materializations()
            .expect("constant ownership validates complete materializations")
    }

    /// 按实际常量读取 identity 查询；Group 不生成第二个 owner。
    #[must_use]
    pub fn materialization_at(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitConstantMaterializationPlan> {
        let plans = self.materializations();
        plans
            .binary_search_by_key(&expression, |plan| plan.descriptor().expression())
            .ok()
            .map(|index| &plans[index])
    }

    /// 检查输入是否属于本产物验证时使用的 typed 分析。
    #[must_use]
    pub fn is_compatible_with(&self, typed: &ConstEnabledTypedUnit) -> bool {
        self.0.is_compatible_with(typed)
    }

    /// 解包专用 recovery；不提供基础 validated owned 的转换。
    #[must_use]
    pub fn into_ownership(self) -> CompilationUnitConstantOwnership {
        self.0
    }
}

/// 校验完整 source/name/environment/typed 身份链后分析 unit 常量 ownership。
/// 即使没有常量声明，专用入口来源也随 recovery 保留，不能绕回基础验证出口。
///
/// ```compile_fail,E0308
/// use lang_frontend::{source::SourceMap, name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
///     type_checking::{ValidatedCompilationUnitTypes, TypeEnvironment}, ownership_checking::check_compilation_unit_constant_ownership};
/// fn reject(sources: &SourceMap, inputs: &[SourceUnitInput<'_>], names: &ValidatedCompilationUnitNames,
///     environment: &TypeEnvironment, typed: &ValidatedCompilationUnitTypes) {
///     check_compilation_unit_constant_ownership(sources, inputs, names, environment, typed);
/// }
/// ```
pub fn check_compilation_unit_constant_ownership(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ConstEnabledTypedUnit,
) -> Result<CompilationUnitConstantOwnership, OwnershipCheckingError> {
    let mut owned = analysis::analyze(sources, inputs, names, environment, typed.types())?;
    owned.provenance.requires_constant_capability = true;
    Ok(CompilationUnitConstantOwnership(owned))
}
