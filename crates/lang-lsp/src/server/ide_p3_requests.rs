//! P3 IDE 语义请求分发：semanticTokens/full、documentHighlight、foldingRange、typeDefinition 与 codeAction。

use lang_frontend::{name_resolution::SourceUnitId, source::SourceId};
use lsp_types::{
    CodeActionParams, CodeActionResponse, DocumentHighlight, DocumentHighlightParams, FoldingRange,
    FoldingRangeParams, GotoDefinitionResponse, SemanticTokensParams, SemanticTokensResult, Uri,
    request::GotoTypeDefinitionParams,
};

use super::{OpenDocument, RequestError, SessionState, map_position_error};
use crate::{
    code_actions, navigation_extras, position_adapter::byte_offset, semantic_tokens,
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

fn single_document_source_unit(document: &OpenDocument) -> Result<SourceUnitId, RequestError> {
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

struct DocContext<'a> {
    snapshot: &'a UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    version: Option<i32>,
}

fn resolve_doc_context<'a>(
    state: &'a SessionState,
    uri: &Uri,
) -> Result<Option<DocContext<'a>>, RequestError> {
    match state {
        SessionState::Unit { session, fallback } => {
            if let Some((snapshot, source_unit, source_id)) = session.query_context(uri) {
                Ok(Some(DocContext {
                    snapshot,
                    source_unit,
                    source_id,
                    version: session.open_document_version(uri),
                }))
            } else if let Some(doc) = fallback.get(uri.as_str()) {
                let source_unit = single_document_source_unit(doc)?;
                Ok(Some(DocContext {
                    snapshot: &doc.analysis,
                    source_unit,
                    source_id: doc.source_id,
                    version: Some(doc.version),
                }))
            } else {
                Ok(None)
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(doc) = documents.get(uri.as_str()) else {
                return Ok(None);
            };
            let source_unit = single_document_source_unit(doc)?;
            Ok(Some(DocContext {
                snapshot: &doc.analysis,
                source_unit,
                source_id: doc.source_id,
                version: Some(doc.version),
            }))
        }
    }
}

pub(super) fn semantic_tokens_full_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<SemanticTokensResult>, RequestError> {
    let params: SemanticTokensParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let Some(ctx) = resolve_doc_context(state, &params.text_document.uri)? else {
        return Ok(None);
    };
    let tokens =
        semantic_tokens::semantic_tokens_full(ctx.snapshot, ctx.source_unit, ctx.source_id)
            .map_err(map_position_error)?;
    Ok(Some(SemanticTokensResult::Tokens(tokens)))
}

pub(super) fn document_highlight_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<Vec<DocumentHighlight>>, RequestError> {
    let params: DocumentHighlightParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let pos = params.text_document_position_params;
    let Some(ctx) = resolve_doc_context(state, &pos.text_document.uri)? else {
        return Ok(None);
    };
    let sources = ctx.snapshot.name_snapshot().sources();
    let Some(offset) =
        byte_offset(sources, ctx.source_id, pos.position).map_err(map_position_error)?
    else {
        return Ok(None);
    };
    let highlights = navigation_extras::document_highlights(
        ctx.snapshot,
        ctx.source_unit,
        ctx.source_id,
        offset,
    )
    .map_err(map_position_error)?;
    Ok(Some(highlights))
}

pub(super) fn folding_range_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<Vec<FoldingRange>>, RequestError> {
    let params: FoldingRangeParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let Some(ctx) = resolve_doc_context(state, &params.text_document.uri)? else {
        return Ok(None);
    };
    let ranges = navigation_extras::folding_ranges(ctx.snapshot, ctx.source_id)
        .map_err(map_position_error)?;
    Ok(Some(ranges))
}

pub(super) fn type_definition_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<GotoDefinitionResponse>, RequestError> {
    let params: GotoTypeDefinitionParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let pos = params.text_document_position_params;
    let uri = &pos.text_document.uri;
    let Some(ctx) = resolve_doc_context(state, uri)? else {
        return Ok(None);
    };
    let sources = ctx.snapshot.name_snapshot().sources();
    let Some(offset) =
        byte_offset(sources, ctx.source_id, pos.position).map_err(map_position_error)?
    else {
        return Ok(None);
    };
    match state {
        SessionState::Unit { session, .. } if session.contains_uri(uri) => {
            navigation_extras::type_definition(
                ctx.snapshot,
                ctx.source_unit,
                ctx.source_id,
                offset,
                |target_su| session.uri_for_source_unit(target_su),
            )
            .map_err(map_position_error)
        }
        _ => navigation_extras::type_definition(
            ctx.snapshot,
            ctx.source_unit,
            ctx.source_id,
            offset,
            |target_su| (target_su == ctx.source_unit).then(|| uri.clone()),
        )
        .map_err(map_position_error),
    }
}

pub(super) fn code_action_response(
    params: serde_json::Value,
    state: &SessionState,
    supports_document_changes: bool,
    supports_code_action_literals: bool,
) -> Result<Option<CodeActionResponse>, RequestError> {
    let params: CodeActionParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    if !supports_code_action_literals {
        return Ok(Some(Vec::new()));
    }
    let uri = &params.text_document.uri;
    if let SessionState::Unit { session, .. } = state
        && session.contains_uri(uri)
        && !session.is_closed_document_fresh(uri)
    {
        return Ok(Some(Vec::new()));
    }
    let Some(ctx) = resolve_doc_context(state, uri)? else {
        return Ok(None);
    };
    let actions = code_actions::code_actions(
        ctx.snapshot,
        ctx.source_unit,
        ctx.source_id,
        uri,
        ctx.version,
        params.range,
        params.context.only.as_deref(),
        supports_document_changes,
    )
    .map_err(map_position_error)?;
    Ok(Some(actions))
}
