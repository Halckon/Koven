//! 独立内存文档直接进入共同 Unit 恢复链；URI 仅作展示和协议映射。

use std::{error::Error, fmt};

use lang_frontend::{
    analysis::{UnitNameAnalysisError, UnitRecoveryAnalysisError, UnitSourceDescriptor,
        analyze_unit_names, analyze_unit_recovery},
    name_resolution::UnitDiagnosticOrderError,
    source::{SourceError, SourceId, SourceMap},
    type_checking::standard_environments,
};

use crate::{
    definition::DefinitionIndexError,
    unit_analysis_snapshot::{UnitAnalysisSnapshot, UnitAnalysisSnapshotError},
};

/// 保留原 SourceId 与共同 snapshot；无旧 checker fallback 或成功文档专用分支。
pub(crate) fn analyze(
    source_name: &str,
    text: &str,
) -> Result<(SourceId, UnitAnalysisSnapshot), AnalysisError> {
    let mut sources = SourceMap::new();
    let source = sources.add_source(source_name, text).map_err(AnalysisError::Source)?;
    let (name_environment, type_environment) = standard_environments();
    let names = analyze_unit_names(
        sources, vec![UnitSourceDescriptor::memory_document(source)], name_environment,
    ).map_err(AnalysisError::Names)?;
    let (typed, owned) = analyze_unit_recovery(&names, &type_environment)
        .map_err(AnalysisError::Recovery)?;
    // names 已聚合 lexer/parser/index/import/name；每个阶段仅加入一次。
    let mut diagnostics = names.names().diagnostics().to_vec();
    diagnostics.extend_from_slice(typed.diagnostics());
    diagnostics.extend_from_slice(owned.diagnostics());
    let snapshot = UnitAnalysisSnapshot::finish(names, Some(typed), Some(owned), diagnostics)
        .map_err(|error| match error {
            UnitAnalysisSnapshotError::Diagnostic(error) => AnalysisError::Diagnostic(error),
            UnitAnalysisSnapshotError::Definition(error) => AnalysisError::Definition(error),
        })?;
    Ok((source, snapshot))
}

/// 单文档 Unit 编排的内部失败。
#[derive(Debug)]
pub(crate) enum AnalysisError {
    Source(SourceError),
    Names(UnitNameAnalysisError),
    Recovery(UnitRecoveryAnalysisError),
    Diagnostic(UnitDiagnosticOrderError),
    Definition(DefinitionIndexError),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "could not register LSP source: {error}"),
            Self::Names(error) => write!(formatter, "unit name analysis failed internally: {error}"),
            Self::Recovery(error) => error.fmt(formatter),
            Self::Diagnostic(error) => write!(formatter, "diagnostic ordering failed internally: {error}"),
            Self::Definition(error) => write!(formatter, "definition indexing failed: {error}"),
        }
    }
}

impl Error for AnalysisError {}

#[cfg(test)]
mod tests {
    use super::analyze;

    #[test]
    fn aggregates_diagnostics_from_every_frontend_stage_in_source_order() {
        let (_, analysis) = analyze(
            "file:///diagnostics.ko",
            "#\nfun names(): Unit { missing }\nfun types(): Unit { val item: String = 1 }\n\
             class Resource()\nfun take(own resource: Resource): Unit {}\n\
             fun moves(own resource: Resource): Unit {\nval first = take(resource)\nval second = take(resource)\n}",
        )
        .expect("analysis must succeed internally");
        let codes = analysis
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();

        assert!(codes.iter().any(|code| code == "L0001"), "{codes:?}");
        assert!(codes.iter().any(|code| code == "L0080"), "{codes:?}");
        assert!(codes.iter().any(|code| code == "L0084"), "{codes:?}");
        assert!(codes.iter().any(|code| code == "L0131"), "{codes:?}");
        assert!(
            analysis
                .diagnostics()
                .windows(2)
                .all(|pair| pair[0].primary_span().start() <= pair[1].primary_span().start())
        );
    }

    #[test]
    fn standard_analysis_environment_accepts_core_containers_and_capabilities() {
        let (_, analysis) = analyze(
            "file:///containers.ko",
            "fun values(): Unit { val items: List<Int> = listOf(1, 2) }\n\
             fun <T: Copyable> copy(own item: T): Unit {}",
        )
        .expect("analysis must succeed internally");

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }
}
