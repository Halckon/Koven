//! source-set base/overlay 生命周期与原子 compilation-unit diagnostic snapshot。

use std::{collections::BTreeMap, error::Error, fmt};

use lang_frontend::{
    analysis::{
        UnitNameAnalysisError, UnitSourceDescriptor, analyze_basic_unit_ownership,
        analyze_unit_names,
    },
    lexer::LexerInternalError,
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNameError, SourceUnitId, UnitDiagnosticOrderError,
    },
    ownership_checking::OwnershipCheckingError,
    parser::ParserInternalError,
    source::{SourceError, SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypeError, check_compilation_unit_types, standard_environments,
    },
};
use lsp_types::{
    Hover, InlayHint, Location, Position, PrepareRenameResponse, Range, Uri, WorkspaceEdit,
};

use crate::{
    definition::{DefinitionIndexError, UnitDefinitionTarget},
    diagnostic_adapter::{DiagnosticMappingError, convert_unit_diagnostics_with_context},
    ide_features,
    position_adapter::{PositionMappingError, byte_offset, span_range},
    rename::{
        RenameError, RenameSourceInput, build_workspace_edit, verify_rename_preserves_bindings,
    },
    source_set::SourceSetConfig,
    unit_analysis_snapshot::{UnitAnalysisSnapshot, UnitAnalysisSnapshotError},
    workspace_manifest,
};

// The independent cfg(test) manual chain keeps its original direct producer imports.
#[cfg(test)]
use crate::definition::UnitDefinitionIndex;
#[cfg(test)]
use crate::diagnostic_adapter::convert_unit_diagnostics;
#[cfg(test)]
use lang_frontend::{
    diagnostic::Diagnostic, name_resolution::ordered_unit_diagnostics,
    ownership_checking::CompilationUnitOwnership, type_checking::CompilationUnitTypes,
};

#[derive(Clone)]
struct Overlay {
    version: i32,
    text: String,
}

/// 一次 source-set 事件完整构造、尚未提交的状态。
pub(crate) struct UnitUpdate {
    overlays: BTreeMap<String, Overlay>,
    snapshot: UnitSnapshot,
    publications: Vec<UnitPublication>,
}

/// 一项按稳定 source key 排序的 diagnostics publication。
pub(crate) struct UnitPublication {
    pub(crate) uri: Uri,
    pub(crate) version: Option<i32>,
    pub(crate) diagnostics: Vec<lsp_types::Diagnostic>,
}

/// immutable base、当前 overlays 与 last-good snapshot。
pub(crate) struct UnitSession {
    config: SourceSetConfig,
    overlays: BTreeMap<String, Overlay>,
    snapshot: UnitSnapshot,
    enrich_diagnostics: bool,
    disk_backed_uris: std::collections::BTreeSet<String>,
}

impl UnitSession {
    /// 从已验证 base 构造首个 snapshot。
    #[allow(dead_code)]
    pub(crate) fn new(config: SourceSetConfig) -> Result<Self, UnitSessionError> {
        Self::new_with_options(config, false, false)
    }

    /// 从已验证 base 构造首个 snapshot，可选开启诊断源码片段上下文与磁盘快照校验。
    pub(crate) fn new_with_options(
        config: SourceSetConfig,
        enrich_diagnostics: bool,
        workspace_manifest: bool,
    ) -> Result<Self, UnitSessionError> {
        let overlays = BTreeMap::new();
        let snapshot = UnitSnapshot::analyze(&config, &overlays)?;
        let disk_backed_uris = config
            .sources()
            .iter()
            .filter(|source| {
                workspace_manifest
                    || workspace_manifest::file_uri_to_path(source.uri())
                        .is_some_and(|path| path.exists())
            })
            .map(|source| source.uri().as_str().to_owned())
            .collect();
        Ok(Self {
            config,
            overlays,
            snapshot,
            enrich_diagnostics,
            disk_backed_uris,
        })
    }

    /// 判断 URI 是否属于当前 source-set base。
    pub(crate) fn contains_uri(&self, uri: &Uri) -> bool {
        self.config
            .sources()
            .iter()
            .any(|source| source.uri().as_str() == uri.as_str())
    }

    /// 返回首个 base-only publication 集合。
    pub(crate) fn publications(&self) -> Result<Vec<UnitPublication>, UnitSessionError> {
        self.snapshot.publications_with_options(
            &self.config,
            &self.overlays,
            self.enrich_diagnostics,
        )
    }

    /// 为已知、尚未打开的 URI 准备 overlay open。
    pub(crate) fn prepare_open(
        &self,
        uri: &Uri,
        version: i32,
        text: String,
    ) -> Result<UnitUpdate, UnitSessionError> {
        self.require_known_uri(uri)?;
        if self.overlays.contains_key(uri.as_str()) {
            return Err(UnitSessionError::Protocol(format!(
                "source-set didOpen duplicated URI {:?}",
                uri.as_str()
            )));
        }
        let mut overlays = self.overlays.clone();
        overlays.insert(uri.as_str().to_owned(), Overlay { version, text });
        self.prepare_update(overlays)
    }

    /// 为已打开 URI 的严格递增 full-document change 准备更新。
    pub(crate) fn prepare_change(
        &self,
        uri: &Uri,
        version: i32,
        text: String,
    ) -> Result<UnitUpdate, UnitSessionError> {
        let Some(current) = self.overlays.get(uri.as_str()) else {
            return Err(UnitSessionError::Protocol(format!(
                "source-set didChange references unopened URI {:?}",
                uri.as_str()
            )));
        };
        if version <= current.version {
            return Err(UnitSessionError::Protocol(format!(
                "source-set didChange version {version} is not newer than {} for {:?}",
                current.version,
                uri.as_str()
            )));
        }
        let mut overlays = self.overlays.clone();
        overlays.insert(uri.as_str().to_owned(), Overlay { version, text });
        self.prepare_update(overlays)
    }

    /// 为已打开 URI 准备回落到 immutable base text。
    pub(crate) fn prepare_close(&self, uri: &Uri) -> Result<UnitUpdate, UnitSessionError> {
        if !self.overlays.contains_key(uri.as_str()) {
            return Err(UnitSessionError::Protocol(format!(
                "source-set didClose references unopened URI {:?}",
                uri.as_str()
            )));
        }
        let mut overlays = self.overlays.clone();
        overlays.remove(uri.as_str());
        self.prepare_update(overlays)
    }

    /// 在全部 publication 成功发送后提交候选状态。
    pub(crate) fn commit(&mut self, update: UnitUpdate) {
        self.overlays = update.overlays;
        self.snapshot = update.snapshot;
    }

    /// 查询当前 last-good snapshot 中的跨文件 definition locations。
    pub(crate) fn definition_locations(
        &self,
        uri: &Uri,
        position: Position,
    ) -> Result<Vec<Location>, UnitDefinitionQueryError> {
        self.snapshot
            .definition_locations(&self.config, uri, position)
    }

    /// 查询当前 last-good snapshot 中的跨文件 references locations。
    pub(crate) fn reference_locations(
        &self,
        uri: &Uri,
        position: Position,
        include_declaration: bool,
    ) -> Result<Vec<Location>, UnitDefinitionQueryError> {
        self.snapshot
            .reference_locations(&self.config, uri, position, include_declaration)
    }

    /// 查询当前 last-good snapshot 中的 hover 信息。
    pub(crate) fn hover(
        &self,
        uri: &Uri,
        position: Position,
    ) -> Result<Option<Hover>, UnitDefinitionQueryError> {
        self.snapshot.hover(&self.config, uri, position)
    }

    /// 查询当前 last-good snapshot 中指定文档范围的 inlay hints。
    pub(crate) fn inlay_hints(
        &self,
        uri: &Uri,
        range: Range,
    ) -> Result<Vec<InlayHint>, UnitDefinitionQueryError> {
        self.snapshot.inlay_hints(&self.config, uri, range)
    }

    /// 校验当前光标位置是否支持跨文件重命名。
    pub(crate) fn prepare_rename(
        &self,
        uri: &Uri,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>, RenameError> {
        self.snapshot.prepare_rename(&self.config, uri, position)
    }

    /// 计算当前 last-good snapshot 中的跨文件安全重命名 `WorkspaceEdit`。
    pub(crate) fn rename(
        &self,
        uri: &Uri,
        position: Position,
        new_name: &str,
        supports_document_changes: bool,
    ) -> Result<Option<WorkspaceEdit>, RenameError> {
        self.snapshot.rename(
            &self.config,
            &self.overlays,
            &self.disk_backed_uris,
            uri,
            position,
            new_name,
            supports_document_changes,
        )
    }

    /// 只为当前打开的 overlay 提供编辑，不能把关闭后的 base 当作编辑器文本。
    pub(crate) fn formatting_edits(
        &self,
        uri: &Uri,
    ) -> Result<Option<Vec<lsp_types::TextEdit>>, String> {
        if !self.overlays.contains_key(uri.as_str()) {
            return Ok(None);
        }
        let source_id = self
            .config
            .sources()
            .iter()
            .position(|source| source.uri() == uri)
            .and_then(|index| self.snapshot.source_ids.get(index))
            .copied()
            .ok_or_else(|| "open document is absent from source-set snapshot".to_owned())?;
        crate::formatting::edits(self.snapshot.analysis.name_snapshot().sources(), source_id)
    }

    fn prepare_update(
        &self,
        overlays: BTreeMap<String, Overlay>,
    ) -> Result<UnitUpdate, UnitSessionError> {
        let snapshot = UnitSnapshot::analyze(&self.config, &overlays)?;
        let publications =
            snapshot.publications_with_options(&self.config, &overlays, self.enrich_diagnostics)?;
        Ok(UnitUpdate {
            overlays,
            snapshot,
            publications,
        })
    }

    fn require_known_uri(&self, uri: &Uri) -> Result<(), UnitSessionError> {
        if self
            .config
            .sources()
            .iter()
            .any(|source| source.uri() == uri)
        {
            Ok(())
        } else {
            Err(UnitSessionError::Protocol(format!(
                "source-set didOpen references unknown URI {:?}",
                uri.as_str()
            )))
        }
    }
}

impl UnitUpdate {
    /// 返回准备完成、按稳定 source key 排序的全部 publications。
    pub(crate) fn publications(&self) -> &[UnitPublication] {
        &self.publications
    }
}

struct UnitSnapshot {
    analysis: UnitAnalysisSnapshot,
    source_ids: Vec<SourceId>,
}

impl UnitSnapshot {
    fn analyze(
        config: &SourceSetConfig,
        overlays: &BTreeMap<String, Overlay>,
    ) -> Result<Self, UnitSessionError> {
        let mut sources = SourceMap::new();
        let mut source_ids = Vec::with_capacity(config.sources().len());
        let mut descriptors = Vec::with_capacity(config.sources().len());
        for source in config.sources() {
            let text = overlays
                .get(source.uri().as_str())
                .map_or(source.text(), |overlay| overlay.text.as_str());
            let source_id = sources.add_source(source.uri().as_str(), text)?;
            descriptors.push(UnitSourceDescriptor::new(
                source.root(),
                source.logical_path(),
                source_id,
            ));
            source_ids.push(source_id);
        }
        let (name_environment, type_environment) = standard_environments();
        let name_snapshot = analyze_unit_names(sources, descriptors, name_environment)?;
        let mut diagnostics = name_snapshot.names().diagnostics().to_vec();
        let (typed, owned) = if let Some(validated_names) = name_snapshot.validated_names() {
            let inputs = name_snapshot.inputs();
            let typed = check_compilation_unit_types(
                name_snapshot.sources(),
                &inputs,
                validated_names,
                &type_environment,
            )?;
            diagnostics.extend_from_slice(typed.diagnostics());
            let outcome = analyze_basic_unit_ownership(
                name_snapshot.sources(),
                &inputs,
                validated_names,
                &type_environment,
                typed,
            )?;
            match outcome.into_result() {
                Ok((typed, owned)) => {
                    diagnostics.extend_from_slice(owned.diagnostics());
                    (Some(typed.into_types()), Some(owned))
                }
                Err(typed) => (Some(*typed), None),
            }
        } else {
            (None, None)
        };
        let analysis = UnitAnalysisSnapshot::finish(name_snapshot, typed, owned, diagnostics)?;
        Ok(Self {
            analysis,
            source_ids,
        })
    }

    #[cfg(test)]
    fn publications(
        &self,
        config: &SourceSetConfig,
        overlays: &BTreeMap<String, Overlay>,
    ) -> Result<Vec<UnitPublication>, UnitSessionError> {
        self.publications_with_options(config, overlays, false)
    }

    fn publications_with_options(
        &self,
        config: &SourceSetConfig,
        overlays: &BTreeMap<String, Overlay>,
        enrich_diagnostics: bool,
    ) -> Result<Vec<UnitPublication>, UnitSessionError> {
        let uris = self
            .source_ids
            .iter()
            .copied()
            .zip(config.sources().iter().map(|source| source.uri().clone()))
            .collect::<Vec<_>>();
        let diagnostics = convert_unit_diagnostics_with_context(
            self.analysis.name_snapshot().sources(),
            &uris,
            self.analysis.diagnostics(),
            enrich_diagnostics,
        )?;
        Ok(config
            .sources()
            .iter()
            .zip(self.source_ids.iter())
            .zip(diagnostics)
            .map(|((source, _source_id), diagnostics)| UnitPublication {
                uri: source.uri().clone(),
                version: overlays
                    .get(source.uri().as_str())
                    .map(|overlay| overlay.version),
                diagnostics,
            })
            .collect())
    }

    fn resolve_source_unit(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
    ) -> Result<Option<(SourceId, SourceUnitId)>, UnitDefinitionQueryError> {
        let Some(source_index) = config
            .sources()
            .iter()
            .position(|source| source.uri() == uri)
        else {
            return Ok(None);
        };
        let source_id = self.source_ids[source_index];
        let source_unit = self
            .analysis
            .name_snapshot()
            .names()
            .index()
            .source_units()
            .iter()
            .find(|source| source.source_id() == source_id)
            .ok_or(UnitDefinitionQueryError::UnknownSource(source_id))?
            .id();
        Ok(Some((source_id, source_unit)))
    }

    fn resolve_source_and_offset(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
        position: Position,
    ) -> Result<Option<(SourceId, SourceUnitId, usize)>, UnitDefinitionQueryError> {
        let Some((source_id, source_unit)) = self.resolve_source_unit(config, uri)? else {
            return Ok(None);
        };
        let Some(offset) =
            byte_offset(self.analysis.name_snapshot().sources(), source_id, position)?
        else {
            return Ok(None);
        };
        Ok(Some((source_id, source_unit, offset)))
    }

    fn target_to_location(
        &self,
        config: &SourceSetConfig,
        target: UnitDefinitionTarget,
    ) -> Result<Location, UnitDefinitionQueryError> {
        let target_source = self
            .analysis
            .name_snapshot()
            .names()
            .index()
            .source_units()
            .get(target.source_unit.index())
            .filter(|source| source.id() == target.source_unit)
            .ok_or(UnitDefinitionQueryError::UnknownSource(
                target.span.source_id(),
            ))?;
        if target.span.source_id() != target_source.source_id() {
            return Err(UnitDefinitionQueryError::MismatchedTargetSource {
                expected: target_source.source_id(),
                actual: target.span.source_id(),
            });
        }
        let target_index = self
            .source_ids
            .iter()
            .position(|source_id| *source_id == target_source.source_id())
            .ok_or(UnitDefinitionQueryError::UnknownSource(
                target_source.source_id(),
            ))?;
        let range = span_range(self.analysis.name_snapshot().sources(), target.span)?;
        Ok(Location::new(
            config.sources()[target_index].uri().clone(),
            range,
        ))
    }

    fn definition_locations(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
        position: Position,
    ) -> Result<Vec<Location>, UnitDefinitionQueryError> {
        let Some((_, source_unit, offset)) =
            self.resolve_source_and_offset(config, uri, position)?
        else {
            return Ok(Vec::new());
        };
        self.analysis
            .definitions()
            .targets_at(source_unit, offset)
            .iter()
            .map(|target| self.target_to_location(config, *target))
            .collect()
    }

    fn reference_locations(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
        position: Position,
        include_declaration: bool,
    ) -> Result<Vec<Location>, UnitDefinitionQueryError> {
        let Some((_, source_unit, offset)) =
            self.resolve_source_and_offset(config, uri, position)?
        else {
            return Ok(Vec::new());
        };
        self.analysis
            .definitions()
            .references_at(source_unit, offset, include_declaration)
            .into_iter()
            .map(|target| self.target_to_location(config, target))
            .collect()
    }

    fn hover(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
        position: Position,
    ) -> Result<Option<Hover>, UnitDefinitionQueryError> {
        let Some((_, source_unit, offset)) =
            self.resolve_source_and_offset(config, uri, position)?
        else {
            return Ok(None);
        };
        Ok(ide_features::hover_at(&self.analysis, source_unit, offset)?)
    }

    fn inlay_hints(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
        range: Range,
    ) -> Result<Vec<InlayHint>, UnitDefinitionQueryError> {
        let Some((source_id, source_unit)) = self.resolve_source_unit(config, uri)? else {
            return Ok(Vec::new());
        };
        Ok(ide_features::inlay_hints_for_source(
            &self.analysis,
            source_unit,
            source_id,
            range,
        )?)
    }

    fn prepare_rename(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>, RenameError> {
        let Some((_, source_unit, offset)) = self
            .resolve_source_and_offset(config, uri, position)
            .map_err(|e| match e {
                UnitDefinitionQueryError::Position(p) => RenameError::Position(p),
                other => RenameError::Internal(other.to_string()),
            })?
        else {
            return Ok(None);
        };
        ide_features::prepare_rename_at(&self.analysis, source_unit, offset)
    }

    #[allow(clippy::too_many_arguments)]
    fn rename(
        &self,
        config: &SourceSetConfig,
        overlays: &BTreeMap<String, Overlay>,
        disk_backed_uris: &std::collections::BTreeSet<String>,
        uri: &Uri,
        position: Position,
        new_name: &str,
        supports_document_changes: bool,
    ) -> Result<Option<WorkspaceEdit>, RenameError> {
        let Some(typed) = self.analysis.typed() else {
            return Err(RenameError::IncompleteAnalysis);
        };
        let Some((_, source_unit, offset)) = self
            .resolve_source_and_offset(config, uri, position)
            .map_err(|e| match e {
                UnitDefinitionQueryError::Position(p) => RenameError::Position(p),
                other => RenameError::Internal(other.to_string()),
            })?
        else {
            return Ok(None);
        };
        let sources = self.analysis.name_snapshot().sources();
        let Some(plan) =
            self.analysis
                .definitions()
                .plan_rename_at(source_unit, offset, sources)?
        else {
            return Ok(None);
        };
        if plan.occurrences.is_empty() {
            return Ok(None);
        }

        // Build current unit inputs and verify post-rename diagnostics and bindings remain sound.
        let index_units = self.analysis.name_snapshot().names().index().source_units();
        let mut verify_inputs = Vec::with_capacity(config.sources().len());
        for (source, &sid) in config.sources().iter().zip(self.source_ids.iter()) {
            let su = index_units
                .iter()
                .find(|u| u.source_id() == sid)
                .ok_or_else(|| RenameError::Internal("source absent from snapshot".to_owned()))?
                .id();
            let text = sources
                .source_text(sid)
                .map_err(|e| RenameError::Internal(e.to_string()))?;
            verify_inputs.push(RenameSourceInput {
                source_id: sid,
                source_unit: su,
                name: source.uri().as_str(),
                root: Some(source.root()),
                logical_path: Some(source.logical_path()),
                text,
            });
        }
        verify_rename_preserves_bindings(
            &verify_inputs,
            self.analysis.name_snapshot().names().diagnostics(),
            typed.diagnostics(),
            &plan,
            new_name,
        )?;

        // Map edits with document version (for open overlays) and verify closed disk files are not stale.
        let mut checked_closed = std::collections::BTreeSet::new();
        let mut edits_by_doc = Vec::with_capacity(plan.occurrences.len());
        for occurrence in plan.occurrences {
            let location = self
                .target_to_location(config, occurrence)
                .map_err(|e| match e {
                    UnitDefinitionQueryError::Position(p) => RenameError::Position(p),
                    other => RenameError::Internal(other.to_string()),
                })?;
            let version = if let Some(overlay) = overlays.get(location.uri.as_str()) {
                Some(overlay.version)
            } else {
                if checked_closed.insert(location.uri.as_str().to_owned()) {
                    let expected = sources
                        .source_text(occurrence.span.source_id())
                        .map_err(|e| RenameError::Internal(e.to_string()))?;
                    let is_disk_backed = disk_backed_uris.contains(location.uri.as_str());
                    if let Some(path) = workspace_manifest::file_uri_to_path(&location.uri) {
                        if is_disk_backed || path.exists() {
                            let disk_text = std::fs::read_to_string(&path).map_err(|_| {
                                RenameError::StaleClosedFile(location.uri.as_str().to_owned())
                            })?;
                            if disk_text != expected {
                                return Err(RenameError::StaleClosedFile(
                                    location.uri.as_str().to_owned(),
                                ));
                            }
                        }
                    } else if is_disk_backed {
                        return Err(RenameError::StaleClosedFile(
                            location.uri.as_str().to_owned(),
                        ));
                    }
                }
                None
            };
            edits_by_doc.push((location.uri, version, location.range));
        }

        Ok(build_workspace_edit(
            edits_by_doc,
            new_name,
            supports_document_changes,
        ))
    }
}

/// source-set lifecycle 的协议拒绝或内部 snapshot 失败。
#[derive(Debug)]
pub(crate) enum UnitSessionError {
    Protocol(String),
    Source(SourceError),
    Lexer(LexerInternalError),
    Parser(ParserInternalError),
    Input(CompilationUnitInputError),
    Name(CompilationUnitNameError),
    Type(CompilationUnitTypeError),
    Ownership(OwnershipCheckingError),
    Diagnostic(UnitDiagnosticOrderError),
    Mapping(DiagnosticMappingError),
    Definition(DefinitionIndexError),
}

impl fmt::Display for UnitSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(message) => formatter.write_str(message),
            Self::Source(error) => write!(formatter, "unit source registration failed: {error}"),
            Self::Lexer(error) => write!(formatter, "unit lexer failed internally: {error}"),
            Self::Parser(error) => write!(formatter, "unit parser failed internally: {error}"),
            Self::Input(error) => write!(formatter, "unit input validation failed: {error}"),
            Self::Name(error) => write!(formatter, "unit name analysis failed internally: {error}"),
            Self::Type(error) => write!(formatter, "unit type analysis failed internally: {error}"),
            Self::Ownership(error) => {
                write!(
                    formatter,
                    "unit ownership analysis failed internally: {error}"
                )
            }
            Self::Diagnostic(error) => {
                write!(formatter, "unit diagnostic ordering failed: {error}")
            }
            Self::Mapping(error) => write!(formatter, "unit diagnostic mapping failed: {error}"),
            Self::Definition(error) => {
                write!(formatter, "unit definition indexing failed: {error}")
            }
        }
    }
}

impl Error for UnitSessionError {}

impl From<UnitAnalysisSnapshotError> for UnitSessionError {
    fn from(error: UnitAnalysisSnapshotError) -> Self {
        match error {
            UnitAnalysisSnapshotError::Diagnostic(error) => Self::Diagnostic(error),
            UnitAnalysisSnapshotError::Definition(error) => Self::Definition(error),
        }
    }
}

impl From<UnitNameAnalysisError> for UnitSessionError {
    fn from(error: UnitNameAnalysisError) -> Self {
        match error {
            UnitNameAnalysisError::Lexer(error) => Self::Lexer(error),
            UnitNameAnalysisError::Parser(error) => Self::Parser(error),
            UnitNameAnalysisError::Input(error) => Self::Input(error),
            UnitNameAnalysisError::Name(error) => Self::Name(error),
        }
    }
}

macro_rules! unit_session_error_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for UnitSessionError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}

unit_session_error_from!(SourceError, Source);
unit_session_error_from!(LexerInternalError, Lexer);
unit_session_error_from!(ParserInternalError, Parser);
unit_session_error_from!(CompilationUnitInputError, Input);
unit_session_error_from!(CompilationUnitNameError, Name);
unit_session_error_from!(CompilationUnitTypeError, Type);
unit_session_error_from!(OwnershipCheckingError, Ownership);
unit_session_error_from!(UnitDiagnosticOrderError, Diagnostic);
unit_session_error_from!(DiagnosticMappingError, Mapping);
unit_session_error_from!(DefinitionIndexError, Definition);

/// definition request 的 position 映射或 snapshot source identity 错误。
#[derive(Debug)]
pub(crate) enum UnitDefinitionQueryError {
    Position(PositionMappingError),
    UnknownSource(SourceId),
    MismatchedTargetSource {
        expected: SourceId,
        actual: SourceId,
    },
}

impl fmt::Display for UnitDefinitionQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Position(error) => write!(formatter, "unit definition position failed: {error}"),
            Self::UnknownSource(source) => {
                write!(
                    formatter,
                    "unit definition references unknown source {source:?}"
                )
            }
            Self::MismatchedTargetSource { expected, actual } => write!(
                formatter,
                "unit definition target source mismatch: expected {expected:?}, got {actual:?}"
            ),
        }
    }
}

impl Error for UnitDefinitionQueryError {}

impl From<PositionMappingError> for UnitDefinitionQueryError {
    fn from(error: PositionMappingError) -> Self {
        Self::Position(error)
    }
}

#[cfg(test)]
mod tests;
