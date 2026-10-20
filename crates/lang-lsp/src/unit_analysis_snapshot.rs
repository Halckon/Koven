//! 同轮 Unit 名称、recovery 产物、完整诊断和 definition facts 的不可变收尾 owner。

use lang_frontend::{
    analysis::UnitNameSnapshot,
    diagnostic::Diagnostic,
    name_resolution::{UnitDiagnosticOrderError, ordered_unit_diagnostics},
    ownership_checking::CompilationUnitOwnership,
    type_checking::CompilationUnitTypes,
};

use crate::definition::{DefinitionIndexError, UnitDefinitionIndex};

/// 只收尾已有同轮分析，不新增资格 gate 或 source-set 生命周期。
pub(crate) struct UnitAnalysisSnapshot {
    // Prefix and recovery facts move together; all spans retain the original SourceMap.
    name_snapshot: UnitNameSnapshot,
    diagnostics: Vec<Diagnostic>,
    _typed: Option<CompilationUnitTypes>,
    _owned: Option<CompilationUnitOwnership>,
    definitions: UnitDefinitionIndex,
}

impl UnitAnalysisSnapshot {
    /// 先排序完整诊断，再构建导航；保留 raw typed/owned，包括错误或 deferred。
    pub(crate) fn finish(
        name_snapshot: UnitNameSnapshot,
        typed: Option<CompilationUnitTypes>,
        owned: Option<CompilationUnitOwnership>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Self, UnitAnalysisSnapshotError> {
        let diagnostics = ordered_unit_diagnostics(
            name_snapshot.sources(),
            name_snapshot.names().index().source_units(),
            &diagnostics,
        )
        .map_err(UnitAnalysisSnapshotError::Diagnostic)?
        .into_iter()
        .cloned()
        .collect();
        let definitions = UnitDefinitionIndex::build(
            name_snapshot.parsed_files(),
            name_snapshot.names(),
            typed.as_ref(),
        )
        .map_err(UnitAnalysisSnapshotError::Definition)?;
        Ok(Self {
            name_snapshot,
            diagnostics,
            _typed: typed,
            _owned: owned,
            definitions,
        })
    }

    /// 借用原名称 owner，不复制或重建来源身份。
    pub(crate) fn name_snapshot(&self) -> &UnitNameSnapshot {
        &self.name_snapshot
    }

    /// 借用已按 source key 和原 span 排序的完整诊断。
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 借用同轮已选 name/type facts 形成的导航索引。
    pub(crate) fn definitions(&self) -> &UnitDefinitionIndex {
        &self.definitions
    }

    #[cfg(test)]
    pub(crate) fn typed(&self) -> Option<&CompilationUnitTypes> {
        self._typed.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn owned(&self) -> Option<&CompilationUnitOwnership> {
        self._owned.as_ref()
    }
}

/// 原排序/导航错误的无损封装；宿主仍映射成其原有错误变体。
#[derive(Debug)]
pub(crate) enum UnitAnalysisSnapshotError {
    Diagnostic(UnitDiagnosticOrderError),
    Definition(DefinitionIndexError),
}
