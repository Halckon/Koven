//! source-set base/overlay 生命周期与原子 compilation-unit diagnostic snapshot。

use std::{collections::BTreeMap, error::Error, fmt};

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexerInternalError, lex},
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNameError, CompilationUnitNames, SourceUnitInput,
        UnitDiagnosticOrderError, index_compilation_unit, ordered_unit_diagnostics,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckingError, check_compilation_unit_ownership,
    },
    parser::{ParsedFile, ParserInternalError, parse_file},
    source::{SourceError, SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypeError, CompilationUnitTypes, check_compilation_unit_types,
        standard_environments,
    },
};
use lsp_types::{Location, Position, Uri};

use crate::{
    definition::{DefinitionIndexError, UnitDefinitionIndex},
    diagnostic_adapter::{DiagnosticMappingError, convert_unit_diagnostics},
    position_adapter::{PositionMappingError, byte_offset, span_range},
    source_set::SourceSetConfig,
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
}

impl UnitSession {
    /// 从已验证 base 构造首个 snapshot。
    pub(crate) fn new(config: SourceSetConfig) -> Result<Self, UnitSessionError> {
        let overlays = BTreeMap::new();
        let snapshot = UnitSnapshot::analyze(&config, &overlays)?;
        Ok(Self {
            config,
            overlays,
            snapshot,
        })
    }

    /// 返回首个 base-only publication 集合。
    pub(crate) fn publications(&self) -> Result<Vec<UnitPublication>, UnitSessionError> {
        self.snapshot.publications(&self.config, &self.overlays)
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

    fn prepare_update(
        &self,
        overlays: BTreeMap<String, Overlay>,
    ) -> Result<UnitUpdate, UnitSessionError> {
        let snapshot = UnitSnapshot::analyze(&self.config, &overlays)?;
        let publications = snapshot.publications(&self.config, &overlays)?;
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
    sources: SourceMap,
    source_ids: Vec<SourceId>,
    diagnostics: Vec<Diagnostic>,
    // Recovery products and definition facts share this snapshot's SourceMap identity. They must
    // be replaced together so a request can never observe spans from another analysis generation.
    _parsed: Vec<ParsedFile>,
    names: CompilationUnitNames,
    _typed: Option<CompilationUnitTypes>,
    _owned: Option<CompilationUnitOwnership>,
    definitions: UnitDefinitionIndex,
}

impl UnitSnapshot {
    fn analyze(
        config: &SourceSetConfig,
        overlays: &BTreeMap<String, Overlay>,
    ) -> Result<Self, UnitSessionError> {
        let mut sources = SourceMap::new();
        let mut source_ids = Vec::with_capacity(config.sources().len());
        let mut parsed = Vec::<ParsedFile>::with_capacity(config.sources().len());
        for source in config.sources() {
            let text = overlays
                .get(source.uri().as_str())
                .map_or(source.text(), |overlay| overlay.text.as_str());
            let source_id = sources.add_source(source.uri().as_str(), text)?;
            let lexed = lex(&sources, source_id)?;
            parsed.push(parse_file(&sources, &lexed)?);
            source_ids.push(source_id);
        }
        let inputs = config
            .sources()
            .iter()
            .zip(source_ids.iter().copied())
            .zip(parsed.iter())
            .map(|((source, source_id), parsed)| {
                SourceUnitInput::new(source.root(), source.logical_path(), source_id, parsed)
            })
            .collect::<Vec<_>>();
        let index = index_compilation_unit(&sources, &inputs)?;
        let (name_environment, type_environment) = standard_environments();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)?;
        let mut diagnostics = names.diagnostics().to_vec();
        let Ok(validated_names) = names.clone().validate() else {
            return Self::finish(sources, source_ids, parsed, names, None, None, diagnostics);
        };
        let typed =
            check_compilation_unit_types(&sources, &inputs, &validated_names, &type_environment)?;
        diagnostics.extend_from_slice(typed.diagnostics());
        let Ok(validated_typed) = typed.clone().validate() else {
            return Self::finish(
                sources,
                source_ids,
                parsed,
                names,
                Some(typed),
                None,
                diagnostics,
            );
        };
        let owned = check_compilation_unit_ownership(
            &sources,
            &inputs,
            &validated_names,
            &type_environment,
            &validated_typed,
        )?;
        diagnostics.extend_from_slice(owned.diagnostics());
        Self::finish(
            sources,
            source_ids,
            parsed,
            names,
            Some(typed),
            Some(owned),
            diagnostics,
        )
    }

    fn finish(
        sources: SourceMap,
        source_ids: Vec<SourceId>,
        parsed: Vec<ParsedFile>,
        names: CompilationUnitNames,
        typed: Option<CompilationUnitTypes>,
        owned: Option<CompilationUnitOwnership>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Self, UnitSessionError> {
        let diagnostics =
            ordered_unit_diagnostics(&sources, names.index().source_units(), &diagnostics)?
                .into_iter()
                .cloned()
                .collect();
        let definitions = UnitDefinitionIndex::build(&parsed, &names, typed.as_ref())?;
        Ok(Self {
            sources,
            source_ids,
            diagnostics,
            _parsed: parsed,
            names,
            _typed: typed,
            _owned: owned,
            definitions,
        })
    }

    fn publications(
        &self,
        config: &SourceSetConfig,
        overlays: &BTreeMap<String, Overlay>,
    ) -> Result<Vec<UnitPublication>, UnitSessionError> {
        let uris = self
            .source_ids
            .iter()
            .copied()
            .zip(config.sources().iter().map(|source| source.uri().clone()))
            .collect::<Vec<_>>();
        let diagnostics = convert_unit_diagnostics(&self.sources, &uris, &self.diagnostics)?;
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

    fn definition_locations(
        &self,
        config: &SourceSetConfig,
        uri: &Uri,
        position: Position,
    ) -> Result<Vec<Location>, UnitDefinitionQueryError> {
        let Some(source_index) = config
            .sources()
            .iter()
            .position(|source| source.uri() == uri)
        else {
            return Ok(Vec::new());
        };
        let source_id = self.source_ids[source_index];
        let source_unit = self
            .names
            .index()
            .source_units()
            .iter()
            .find(|source| source.source_id() == source_id)
            .ok_or(UnitDefinitionQueryError::UnknownSource(source_id))?
            .id();
        let Some(offset) = byte_offset(&self.sources, source_id, position)? else {
            return Ok(Vec::new());
        };
        self.definitions
            .targets_at(source_unit, offset)
            .iter()
            .map(|target| {
                let target_source = self
                    .names
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
                let range = span_range(&self.sources, target.span)?;
                Ok(Location::new(
                    config.sources()[target_index].uri().clone(),
                    range,
                ))
            })
            .collect()
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
