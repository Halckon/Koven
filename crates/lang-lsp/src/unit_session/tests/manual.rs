//! 冻结 main08c7b096 的旧 LSP 全链；只在 cfg(test) 中作为宿主差分参考。
use super::*;
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        CompilationUnitNames, SourceUnitInput, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
};

pub(super) struct ManualSnapshot {
    sources: SourceMap,
    source_ids: Vec<SourceId>,
    diagnostics: Vec<Diagnostic>,
    // Recovery products and definition facts share this snapshot's SourceMap identity. They must
    // be replaced together so a request can never observe spans from another analysis generation.
    _parsed: Vec<ParsedFile>,
    names: CompilationUnitNames,
    pub(super) _typed: Option<CompilationUnitTypes>,
    pub(super) _owned: Option<CompilationUnitOwnership>,
    definitions: UnitDefinitionIndex,
}

impl ManualSnapshot {
    pub(super) fn analyze(
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

    pub(super) fn publications(
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

    pub(super) fn definition_locations(
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
