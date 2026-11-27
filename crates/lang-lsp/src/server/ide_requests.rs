//! IDE 语义请求分发：references、hover、inlayHint、prepareRename 与 rename。

use lsp_types::{
    Hover, HoverParams, InlayHint, InlayHintParams, Location, PrepareRenameResponse,
    ReferenceParams, RenameParams, TextDocumentPositionParams, Uri, WorkspaceEdit,
};

use super::{OpenDocument, RequestError, SessionState, map_position_error, map_unit_query_error};
use crate::{
    ide_features,
    position_adapter::{byte_offset, span_range},
    rename::{
        RenameError, RenameSourceInput, build_workspace_edit, is_valid_rename_identifier,
        verify_rename_preserves_bindings,
    },
};

fn single_document_source_unit(
    document: &OpenDocument,
) -> Result<lang_frontend::name_resolution::SourceUnitId, RequestError> {
    let source_id = document.source_id;
    document
        .analysis
        .name_snapshot()
        .names()
        .index()
        .source_units()
        .iter()
        .find(|source| source.source_id() == source_id)
        .ok_or_else(|| RequestError::Internal("document source is absent from unit".to_owned()))
        .map(|source| source.id())
}

fn single_document_offset(
    document: &OpenDocument,
    position: lsp_types::Position,
) -> Result<Option<(lang_frontend::name_resolution::SourceUnitId, usize)>, RequestError> {
    let source_unit = single_document_source_unit(document)?;
    let sources = document.analysis.name_snapshot().sources();
    let Some(offset) =
        byte_offset(sources, document.source_id, position).map_err(map_position_error)?
    else {
        return Ok(None);
    };
    Ok(Some((source_unit, offset)))
}

fn single_document_reference_locations(
    document: &OpenDocument,
    uri: &Uri,
    position: lsp_types::Position,
    include_declaration: bool,
) -> Result<Vec<Location>, RequestError> {
    let Some((source_unit, offset)) = single_document_offset(document, position)? else {
        return Ok(Vec::new());
    };
    let sources = document.analysis.name_snapshot().sources();
    document
        .analysis
        .definitions()
        .references_at(source_unit, offset, include_declaration)
        .into_iter()
        .map(|target| {
            span_range(sources, target.span)
                .map(|range| Location::new(uri.clone(), range))
                .map_err(|error| RequestError::Internal(error.to_string()))
        })
        .collect()
}

pub(super) fn references_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<Vec<Location>>, RequestError> {
    let params: ReferenceParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let pos = params.text_document_position;
    let include_decl = params.context.include_declaration;
    let locations = match state {
        SessionState::Unit { session, fallback } => {
            if session.contains_uri(&pos.text_document.uri) {
                session
                    .reference_locations(&pos.text_document.uri, pos.position, include_decl)
                    .map_err(map_unit_query_error)?
            } else if let Some(document) = fallback.get(pos.text_document.uri.as_str()) {
                single_document_reference_locations(
                    document,
                    &pos.text_document.uri,
                    pos.position,
                    include_decl,
                )?
            } else {
                return Ok(None);
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(document) = documents.get(pos.text_document.uri.as_str()) else {
                return Ok(None);
            };
            single_document_reference_locations(
                document,
                &pos.text_document.uri,
                pos.position,
                include_decl,
            )?
        }
    };
    Ok((!locations.is_empty()).then_some(locations))
}

pub(super) fn hover_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<Hover>, RequestError> {
    let params: HoverParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let pos = params.text_document_position_params;
    match state {
        SessionState::Unit { session, fallback } => {
            if session.contains_uri(&pos.text_document.uri) {
                session
                    .hover(&pos.text_document.uri, pos.position)
                    .map_err(map_unit_query_error)
            } else if let Some(document) = fallback.get(pos.text_document.uri.as_str()) {
                let Some((source_unit, offset)) = single_document_offset(document, pos.position)?
                else {
                    return Ok(None);
                };
                ide_features::hover_at(&document.analysis, source_unit, offset)
                    .map_err(map_position_error)
            } else {
                Ok(None)
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(document) = documents.get(pos.text_document.uri.as_str()) else {
                return Ok(None);
            };
            let Some((source_unit, offset)) = single_document_offset(document, pos.position)?
            else {
                return Ok(None);
            };
            ide_features::hover_at(&document.analysis, source_unit, offset)
                .map_err(map_position_error)
        }
    }
}

pub(super) fn inlay_hint_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<Vec<InlayHint>>, RequestError> {
    let params: InlayHintParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let hints = match state {
        SessionState::Unit { session, fallback } => {
            if session.contains_uri(&params.text_document.uri) {
                session
                    .inlay_hints(&params.text_document.uri, params.range)
                    .map_err(map_unit_query_error)?
            } else if let Some(document) = fallback.get(params.text_document.uri.as_str()) {
                let source_unit = single_document_source_unit(document)?;
                ide_features::inlay_hints_for_source(
                    &document.analysis,
                    source_unit,
                    document.source_id,
                    params.range,
                )
                .map_err(map_position_error)?
            } else {
                return Ok(None);
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(document) = documents.get(params.text_document.uri.as_str()) else {
                return Ok(None);
            };
            let source_unit = single_document_source_unit(document)?;
            ide_features::inlay_hints_for_source(
                &document.analysis,
                source_unit,
                document.source_id,
                params.range,
            )
            .map_err(map_position_error)?
        }
    };
    Ok(Some(hints))
}

pub(super) fn prepare_rename_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<PrepareRenameResponse>, RequestError> {
    let pos: TextDocumentPositionParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    match state {
        SessionState::Unit { session, fallback } => {
            if session.contains_uri(&pos.text_document.uri) {
                session
                    .prepare_rename(&pos.text_document.uri, pos.position)
                    .map_err(map_rename_error)
            } else if let Some(document) = fallback.get(pos.text_document.uri.as_str()) {
                let Some((source_unit, offset)) = single_document_offset(document, pos.position)?
                else {
                    return Ok(None);
                };
                ide_features::prepare_rename_at(&document.analysis, source_unit, offset)
                    .map_err(map_rename_error)
            } else {
                Ok(None)
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(document) = documents.get(pos.text_document.uri.as_str()) else {
                return Ok(None);
            };
            let Some((source_unit, offset)) = single_document_offset(document, pos.position)?
            else {
                return Ok(None);
            };
            ide_features::prepare_rename_at(&document.analysis, source_unit, offset)
                .map_err(map_rename_error)
        }
    }
}

fn map_rename_error(error: RenameError) -> RequestError {
    match error {
        RenameError::InvalidIdentifier(_)
        | RenameError::IncompleteAnalysis
        | RenameError::SharedOverloadReference
        | RenameError::AliasReference
        | RenameError::BindingConflict
        | RenameError::StaleClosedFile(_) => RequestError::InvalidParams(error.to_string()),
        RenameError::Position(p) => map_position_error(p),
        RenameError::Internal(message) => RequestError::Internal(message),
    }
}

fn single_document_rename(
    document: &OpenDocument,
    uri: &Uri,
    position: lsp_types::Position,
    new_name: &str,
    supports_document_changes: bool,
) -> Result<Option<WorkspaceEdit>, RequestError> {
    let Some(typed) = document.analysis.typed() else {
        return Err(map_rename_error(RenameError::IncompleteAnalysis));
    };
    let Some((source_unit, offset)) = single_document_offset(document, position)? else {
        return Ok(None);
    };
    let sources = document.analysis.name_snapshot().sources();
    let Some(plan) = document
        .analysis
        .definitions()
        .plan_rename_at(source_unit, offset, sources)
        .map_err(map_rename_error)?
    else {
        return Ok(None);
    };
    if plan.occurrences.is_empty() {
        return Ok(None);
    }
    let text = sources
        .source_text(document.source_id)
        .map_err(|e| RequestError::Internal(e.to_string()))?;
    let inputs = [RenameSourceInput {
        source_id: document.source_id,
        source_unit,
        name: uri.as_str(),
        root: None,
        logical_path: None,
        text,
    }];
    verify_rename_preserves_bindings(
        &inputs,
        document.analysis.name_snapshot().names().diagnostics(),
        typed.diagnostics(),
        &plan,
        new_name,
    )
    .map_err(map_rename_error)?;
    let edits_by_doc = plan
        .occurrences
        .into_iter()
        .map(|occ| {
            span_range(sources, occ.span)
                .map(|range| (uri.clone(), Some(document.version), range))
                .map_err(|e| RequestError::Internal(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(build_workspace_edit(
        edits_by_doc,
        new_name,
        supports_document_changes,
    ))
}

pub(super) fn rename_response(
    params: serde_json::Value,
    state: &SessionState,
    supports_document_changes: bool,
) -> Result<Option<WorkspaceEdit>, RequestError> {
    let params: RenameParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    if !is_valid_rename_identifier(&params.new_name) {
        return Err(map_rename_error(RenameError::InvalidIdentifier(
            params.new_name,
        )));
    }
    let pos = params.text_document_position;
    match state {
        SessionState::Unit { session, fallback } => {
            if session.contains_uri(&pos.text_document.uri) {
                session
                    .rename(
                        &pos.text_document.uri,
                        pos.position,
                        &params.new_name,
                        supports_document_changes,
                    )
                    .map_err(map_rename_error)
            } else if let Some(document) = fallback.get(pos.text_document.uri.as_str()) {
                single_document_rename(
                    document,
                    &pos.text_document.uri,
                    pos.position,
                    &params.new_name,
                    supports_document_changes,
                )
            } else {
                Ok(None)
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(document) = documents.get(pos.text_document.uri.as_str()) else {
                return Ok(None);
            };
            single_document_rename(
                document,
                &pos.text_document.uri,
                pos.position,
                &params.new_name,
                supports_document_changes,
            )
        }
    }
}
