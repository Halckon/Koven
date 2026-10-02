//! 签名产物的来源兼容性；既有 public gate 与 sealed handoff 共用同一比较。

use std::sync::Arc;

use crate::{
    name_resolution::{
        CompilationUnitIndex, SourceUnitInput, ValidatedCompilationUnitNames,
        index_compilation_unit,
    },
    source::SourceMap,
    type_checking::TypeEnvironment,
};

use super::CompilationUnitSignatures;

impl CompilationUnitSignatures {
    /// 检查本签名产物是否来自给定 inputs、名称分析与类型环境身份链。
    #[must_use]
    pub fn is_compatible_with(
        &self,
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
        names: &ValidatedCompilationUnitNames,
        environment: &TypeEnvironment,
    ) -> bool {
        index_compilation_unit(sources, inputs)
            .is_ok_and(|index| self.is_compatible_with_index(&index, names, environment))
    }

    /// 复用调用方已核验的 index，保留完整签名来源身份比较。
    pub(crate) fn is_compatible_with_index(
        &self,
        index: &CompilationUnitIndex,
        names: &ValidatedCompilationUnitNames,
        environment: &TypeEnvironment,
    ) -> bool {
        let unit_names = names.names();
        index == &self.provenance.input_index
            && unit_names.index() == &self.provenance.input_index
            && Arc::ptr_eq(&self.provenance.environment_owner, environment.owner())
            && self.provenance.name_analysis_owners.len() == unit_names.source_units().len()
            && self
                .provenance
                .name_analysis_owners
                .iter()
                .zip(unit_names.source_units())
                .all(|(owner, source)| Arc::ptr_eq(owner, source.resolution().analysis_owner()))
    }

    /// 判断两个签名产物是否来自同一次签名分析；克隆产物保持该身份。
    #[must_use]
    pub fn is_same_analysis(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.provenance.analysis_owner,
            &other.provenance.analysis_owner,
        )
    }
}
