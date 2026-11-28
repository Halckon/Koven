//! LSP lifecycle、打开文档状态与诊断发布。

mod ide_p3_requests;
mod ide_requests;
mod notifications;

use std::{collections::BTreeMap, error::Error, fmt};

use lang_frontend::source::SourceId;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionProviderCapability, CompletionOptions,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, FoldingRangeProviderCapability, GotoDefinitionParams,
    GotoDefinitionResponse, HoverProviderCapability, ImplementationProviderCapability,
    InitializeParams, InitializeResult, Location, LogMessageParams, MessageType, OneOf,
    PositionEncodingKind, PublishDiagnosticsParams, RenameOptions, SaveOptions,
    SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, SignatureHelpOptions, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
    TypeDefinitionProviderCapability, Uri,
    notification::{
        DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
        DidSaveTextDocument, LogMessage, Notification as LspNotification, PublishDiagnostics,
    },
    request::{
        CodeActionRequest, Completion, DocumentHighlightRequest, DocumentSymbolRequest,
        FoldingRangeRequest, Formatting, GotoDefinition, GotoImplementation, GotoTypeDefinition,
        HoverRequest, InlayHintRequest, PrepareRenameRequest, References, Rename,
        Request as LspRequest, SemanticTokensFullDeltaRequest, SemanticTokensFullRequest,
        SignatureHelpRequest, WorkspaceSymbolRequest,
    },
};

use crate::{
    analysis::{AnalysisError, analyze},
    diagnostic_adapter::{DiagnosticMappingError, convert_unit_diagnostics_with_context},
    position_adapter::{PositionMappingError, apply_content_changes, byte_offset, span_range},
    semantic_tokens::SemanticTokenCache,
    source_set::SourceSetConfig,
    unit_analysis_snapshot::UnitAnalysisSnapshot,
    unit_session::{UnitDefinitionQueryError, UnitPublication, UnitSession, UnitSessionError},
    workspace_manifest,
};

/// `lang-lsp` 运行选项；默认全关闭以精确保留 ADR-0021 既有行为。
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ServerOptions {
    pub(crate) workspace_manifest: bool,
    pub(crate) diagnostic_context: bool,
}

/// 运行一个已建立 transport 的 Koven LSP 会话（默认选项）。
#[allow(dead_code)]
pub(crate) fn run(connection: Connection) -> Result<(), ServerError> {
    run_with_options(connection, ServerOptions::default())
}

/// 以显式选项运行 Koven LSP 会话。
pub(crate) fn run_with_options(
    connection: Connection,
    options: ServerOptions,
) -> Result<(), ServerError> {
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let initialize_params: InitializeParams = match serde_json::from_value(initialize_params) {
        Ok(params) => params,
        Err(error) => {
            send_response(
                &connection,
                Response::new_err(
                    initialize_id,
                    ErrorCode::InvalidParams as i32,
                    format!("invalid initialize params: {error}"),
                ),
            )?;
            return Ok(());
        }
    };
    let supports_document_changes = initialize_params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|w| w.workspace_edit.as_ref())
        .and_then(|e| e.document_changes)
        .unwrap_or(false);
    let hierarchical_document_symbols = initialize_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|t| t.document_symbol.as_ref())
        .and_then(|d| d.hierarchical_document_symbol_support)
        .unwrap_or(false);
    let supports_code_action_literals = initialize_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|t| t.code_action.as_ref())
        .and_then(|ca| ca.code_action_literal_support.as_ref())
        .is_some();
    let snippet_support = initialize_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|t| t.completion.as_ref())
        .and_then(|c| c.completion_item.as_ref())
        .and_then(|item| item.snippet_support)
        .unwrap_or(false);
    let mut token_cache = SemanticTokenCache::default();
    let source_set = match SourceSetConfig::from_initialization_options(
        initialize_params.initialization_options.as_ref(),
    ) {
        Ok(source_set) => source_set,
        Err(error) => {
            send_response(
                &connection,
                Response::new_err(
                    initialize_id,
                    ErrorCode::InvalidParams as i32,
                    error.to_string(),
                ),
            )?;
            return Ok(());
        }
    };
    let explicit_source_set = source_set.is_some();
    let source_set = match source_set {
        Some(config) => Some(config),
        None if options.workspace_manifest => {
            match workspace_manifest::discover_source_set(&initialize_params) {
                Ok(discovered) => discovered,
                Err(error) => {
                    let _ = send_log(
                        &connection,
                        format!("workspace project.toml discovery skipped: {error}"),
                    );
                    None
                }
            }
        }
        None => None,
    };
    let (mut state, initial_publications) = match source_set {
        Some(config) => match UnitSession::new_with_options(
            config,
            options.diagnostic_context,
            options.workspace_manifest,
        ) {
            Ok(session) => match session.publications() {
                Ok(publications) => (
                    SessionState::Unit {
                        session: Box::new(session),
                        fallback: BTreeMap::new(),
                    },
                    Some(publications),
                ),
                Err(error) => {
                    send_response(
                        &connection,
                        Response::new_err(
                            initialize_id,
                            ErrorCode::InternalError as i32,
                            error.to_string(),
                        ),
                    )?;
                    return Ok(());
                }
            },
            Err(error) => {
                send_response(
                    &connection,
                    Response::new_err(
                        initialize_id,
                        ErrorCode::InternalError as i32,
                        error.to_string(),
                    ),
                )?;
                return Ok(());
            }
        },
        None => (SessionState::SingleDocument(BTreeMap::new()), None),
    };
    let initialize_result = InitializeResult {
        capabilities: capabilities(supports_code_action_literals, options.workspace_manifest),
        server_info: Some(ServerInfo {
            name: "koven-lsp".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(initialize_result)?)?;
    if let Some(publications) = initial_publications {
        publish_unit(&connection, &publications)
            .map_err(|error| ServerError::Internal(error.to_string()))?;
    }

    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                let Request { id, method, params } = request;
                macro_rules! dispatch_json {
                    ($result:expr) => {{
                        let response = match $result {
                            Ok(value) => Response::new_ok(id, serde_json::to_value(value)?),
                            Err(RequestError::InvalidParams(message)) => {
                                Response::new_err(id, ErrorCode::InvalidParams as i32, message)
                            }
                            Err(RequestError::Internal(message)) => {
                                Response::new_err(id, ErrorCode::InternalError as i32, message)
                            }
                        };
                        send_response(&connection, response)?;
                    }};
                }
                match method.as_str() {
                    GotoDefinition::METHOD => dispatch_json!(definition_response(params, &state)),
                    References::METHOD => {
                        dispatch_json!(ide_requests::references_response(params, &state))
                    }
                    HoverRequest::METHOD => {
                        dispatch_json!(ide_requests::hover_response(params, &state))
                    }
                    InlayHintRequest::METHOD => {
                        dispatch_json!(ide_requests::inlay_hint_response(params, &state))
                    }
                    Completion::METHOD => {
                        dispatch_json!(ide_requests::completion_response(
                            params,
                            &state,
                            snippet_support
                        ))
                    }
                    SignatureHelpRequest::METHOD => {
                        dispatch_json!(ide_requests::signature_help_response(params, &state))
                    }
                    DocumentSymbolRequest::METHOD => {
                        dispatch_json!(ide_requests::document_symbol_response(
                            params,
                            &state,
                            hierarchical_document_symbols
                        ))
                    }
                    WorkspaceSymbolRequest::METHOD => {
                        dispatch_json!(ide_requests::workspace_symbol_response(params, &state))
                    }
                    PrepareRenameRequest::METHOD => {
                        dispatch_json!(ide_requests::prepare_rename_response(params, &state))
                    }
                    Rename::METHOD => dispatch_json!(ide_requests::rename_response(
                        params,
                        &state,
                        supports_document_changes
                    )),
                    Formatting::METHOD => dispatch_json!(formatting_response(params, &state)),
                    SemanticTokensFullRequest::METHOD => {
                        dispatch_json!(ide_p3_requests::semantic_tokens_full_response(
                            params,
                            &state,
                            &mut token_cache
                        ))
                    }
                    SemanticTokensFullDeltaRequest::METHOD => {
                        dispatch_json!(ide_p3_requests::semantic_tokens_full_delta_response(
                            params,
                            &state,
                            &mut token_cache
                        ))
                    }
                    DocumentHighlightRequest::METHOD => {
                        dispatch_json!(ide_p3_requests::document_highlight_response(params, &state))
                    }
                    FoldingRangeRequest::METHOD => {
                        dispatch_json!(ide_p3_requests::folding_range_response(params, &state))
                    }
                    GotoTypeDefinition::METHOD => {
                        dispatch_json!(ide_p3_requests::type_definition_response(params, &state))
                    }
                    GotoImplementation::METHOD => {
                        dispatch_json!(ide_p3_requests::implementation_response(params, &state))
                    }
                    CodeActionRequest::METHOD => {
                        dispatch_json!(ide_p3_requests::code_action_response(
                            params,
                            &state,
                            supports_document_changes,
                            supports_code_action_literals
                        ))
                    }
                    _ => {
                        send_response(
                            &connection,
                            Response::new_err(
                                id,
                                ErrorCode::MethodNotFound as i32,
                                format!("unsupported request method {method:?}"),
                            ),
                        )?;
                    }
                }
            }
            Message::Notification(notification) => {
                if notification.method.as_str() == DidCloseTextDocument::METHOD
                    && let Some(uri) = notification_document_uri(&notification)
                {
                    token_cache.remove(uri.as_str());
                }
                if let Err(error) = handle_notification(
                    &connection,
                    notification,
                    &mut state,
                    options,
                    &initialize_params,
                    explicit_source_set,
                ) {
                    send_log(&connection, error.to_string())?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn capabilities(
    supports_code_action_literals: bool,
    workspace_manifest: bool,
) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(PositionEncodingKind::UTF16),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(if workspace_manifest {
                    TextDocumentSyncKind::INCREMENTAL
                } else {
                    TextDocumentSyncKind::FULL
                }),
                will_save: None,
                will_save_wait_until: None,
                save: workspace_manifest.then_some(TextDocumentSyncSaveOptions::SaveOptions(
                    SaveOptions {
                        include_text: Some(false),
                    },
                )),
            },
        )),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        inlay_hint_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_owned()]),
            ..CompletionOptions::default()
        }),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_owned(), ",".to_owned()]),
            ..SignatureHelpOptions::default()
        }),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
        code_action_provider: supports_code_action_literals.then(|| {
            CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![
                    CodeActionKind::QUICKFIX,
                    CodeActionKind::REFACTOR_REWRITE,
                ]),
                ..CodeActionOptions::default()
            })
        }),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: crate::semantic_tokens::legend(),
                full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                range: None,
                ..SemanticTokensOptions::default()
            },
        )),
        ..ServerCapabilities::default()
    }
}

struct OpenDocument {
    version: i32,
    source_id: SourceId,
    analysis: UnitAnalysisSnapshot,
}

enum SessionState {
    SingleDocument(BTreeMap<String, OpenDocument>),
    Unit {
        session: Box<UnitSession>,
        fallback: BTreeMap<String, OpenDocument>,
    },
}

impl OpenDocument {
    fn analyze(uri: &Uri, version: i32, text: &str) -> Result<Self, AnalysisError> {
        let (source_id, analysis) = analyze(uri.as_str(), text)?;
        Ok(Self {
            version,
            source_id,
            analysis,
        })
    }

    fn version(&self) -> i32 {
        self.version
    }

    fn text(&self) -> Option<&str> {
        self.analysis
            .name_snapshot()
            .sources()
            .source_text(self.source_id)
            .ok()
    }
}

fn handle_notification(
    connection: &Connection,
    notification: Notification,
    state: &mut SessionState,
    options: ServerOptions,
    initialize_params: &InitializeParams,
    explicit_source_set: bool,
) -> Result<(), NotificationError> {
    if notification.method.as_str() == DidChangeWatchedFiles::METHOD {
        if options.workspace_manifest && !explicit_source_set {
            notifications::handle_watched_files(
                connection,
                notification,
                state,
                options,
                initialize_params,
            )?;
        }
        return Ok(());
    }
    match state {
        SessionState::SingleDocument(documents) => {
            handle_single_document_notification_with_options(
                connection,
                notification,
                documents,
                options.diagnostic_context,
                options.workspace_manifest,
            )
        }
        SessionState::Unit { session, fallback } => {
            if options.workspace_manifest
                && let Some(uri) = notification_document_uri(&notification)
                && !session.contains_uri(&uri)
            {
                return handle_single_document_notification_with_options(
                    connection,
                    notification,
                    fallback,
                    options.diagnostic_context,
                    options.workspace_manifest,
                );
            }
            handle_unit_notification_with_options(
                connection,
                notification,
                session,
                options.workspace_manifest,
            )
        }
    }
}

fn notification_document_uri(notification: &Notification) -> Option<Uri> {
    let uri_value = notification.params.get("textDocument")?.get("uri")?;
    serde_json::from_value(uri_value.clone()).ok()
}

#[cfg(test)]
fn handle_single_document_notification(
    connection: &Connection,
    notification: Notification,
    documents: &mut BTreeMap<String, OpenDocument>,
) -> Result<(), NotificationError> {
    handle_single_document_notification_with_options(
        connection,
        notification,
        documents,
        false,
        false,
    )
}

fn handle_single_document_notification_with_options(
    connection: &Connection,
    notification: Notification,
    documents: &mut BTreeMap<String, OpenDocument>,
    enrich_diagnostics: bool,
    allow_incremental: bool,
) -> Result<(), NotificationError> {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
            let uri = params.text_document.uri;
            let document = OpenDocument::analyze(
                &uri,
                params.text_document.version,
                &params.text_document.text,
            )?;
            publish_document(connection, &uri, &document, enrich_diagnostics)?;
            documents.insert(uri.as_str().to_owned(), document);
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            let Some(existing) = documents.get(params.text_document.uri.as_str()) else {
                return Ok(());
            };
            let new_text = if allow_incremental {
                let current_text = existing
                    .analysis
                    .name_snapshot()
                    .sources()
                    .source_text(existing.source_id)
                    .map_err(|_| NotificationError::ExpectedSingleFullChange)?;
                apply_content_changes(current_text, &params.content_changes)
                    .map_err(|_| NotificationError::ExpectedSingleFullChange)?
            } else {
                let [change] = params.content_changes.as_slice() else {
                    return Err(NotificationError::ExpectedSingleFullChange);
                };
                if change.range.is_some() || change.range_length.is_some() {
                    return Err(NotificationError::ExpectedSingleFullChange);
                }
                change.text.clone()
            };
            let document = OpenDocument::analyze(
                &params.text_document.uri,
                params.text_document.version,
                &new_text,
            )?;
            publish_document(
                connection,
                &params.text_document.uri,
                &document,
                enrich_diagnostics,
            )?;
            documents.insert(params.text_document.uri.as_str().to_owned(), document);
        }
        DidCloseTextDocument::METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
            documents.remove(params.text_document.uri.as_str());
            publish(connection, params.text_document.uri, None, Vec::new())?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
fn handle_unit_notification(
    connection: &Connection,
    notification: Notification,
    session: &mut UnitSession,
) -> Result<(), NotificationError> {
    handle_unit_notification_with_options(connection, notification, session, false)
}

fn handle_unit_notification_with_options(
    connection: &Connection,
    notification: Notification,
    session: &mut UnitSession,
    allow_incremental: bool,
) -> Result<(), NotificationError> {
    let update = match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
            session.prepare_open(
                &params.text_document.uri,
                params.text_document.version,
                params.text_document.text,
            )?
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            let new_text = if allow_incremental {
                let Some(current_text) = session.overlay_text(&params.text_document.uri) else {
                    return Err(NotificationError::Unit(UnitSessionError::Protocol(
                        format!(
                            "source-set didChange references unopened URI {:?}",
                            params.text_document.uri.as_str()
                        ),
                    )));
                };
                apply_content_changes(current_text, &params.content_changes)
                    .map_err(|_| NotificationError::ExpectedSingleFullChange)?
            } else {
                let [change] = params.content_changes.as_slice() else {
                    return Err(NotificationError::ExpectedSingleFullChange);
                };
                if change.range.is_some() || change.range_length.is_some() {
                    return Err(NotificationError::ExpectedSingleFullChange);
                }
                change.text.clone()
            };
            session.prepare_change(
                &params.text_document.uri,
                params.text_document.version,
                new_text,
            )?
        }
        DidSaveTextDocument::METHOD => {
            let params: DidSaveTextDocumentParams = serde_json::from_value(notification.params)?;
            let uri = &params.text_document.uri;
            let saved_text = params
                .text
                .or_else(|| session.overlay_text(uri).map(str::to_owned))
                .or_else(|| {
                    workspace_manifest::file_uri_to_path(uri)
                        .and_then(|p| std::fs::read_to_string(p).ok())
                });
            let Some(saved_text) = saved_text else {
                return Ok(());
            };
            let Some(update) = session.prepare_base_refresh(uri, saved_text)? else {
                return Ok(());
            };
            update
        }
        DidCloseTextDocument::METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
            session.prepare_close(&params.text_document.uri)?
        }
        _ => return Ok(()),
    };
    publish_unit(connection, update.publications())?;
    session.commit(update);
    Ok(())
}

fn publish_unit(
    connection: &Connection,
    publications: &[UnitPublication],
) -> Result<(), NotificationError> {
    for publication in publications {
        publish(
            connection,
            publication.uri.clone(),
            publication.version,
            publication.diagnostics.clone(),
        )?;
    }
    Ok(())
}

fn publish_document(
    connection: &Connection,
    uri: &Uri,
    document: &OpenDocument,
    enrich_diagnostics: bool,
) -> Result<(), NotificationError> {
    let diagnostics = convert_unit_diagnostics_with_context(
        document.analysis.name_snapshot().sources(),
        &[(document.source_id, uri.clone())],
        document.analysis.diagnostics(),
        enrich_diagnostics,
        document
            .analysis
            .name_snapshot()
            .names()
            .builtin_display_source(),
    )?
    .into_iter()
    .next()
    .ok_or(DiagnosticMappingError::UnknownSource(document.source_id))?;
    publish(connection, uri.clone(), Some(document.version), diagnostics)
}

fn map_unit_query_error(error: UnitDefinitionQueryError) -> RequestError {
    match error {
        UnitDefinitionQueryError::Position(PositionMappingError::InsideSurrogatePair) => {
            RequestError::InvalidParams("position splits a UTF-16 surrogate pair".to_owned())
        }
        other => RequestError::Internal(other.to_string()),
    }
}

fn map_position_error(error: PositionMappingError) -> RequestError {
    match error {
        PositionMappingError::InsideSurrogatePair => {
            RequestError::InvalidParams("position splits a UTF-16 surrogate pair".to_owned())
        }
        other => RequestError::Internal(other.to_string()),
    }
}

fn formatting_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<Vec<lsp_types::TextEdit>>, RequestError> {
    let params: lsp_types::DocumentFormattingParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    // 工具链采用 frontend 固定样式，客户端 options 不创建另一套格式语义。
    match state {
        SessionState::Unit { session, fallback } => {
            if session.contains_uri(&params.text_document.uri) {
                session.formatting_edits(&params.text_document.uri)
            } else if let Some(document) = fallback.get(params.text_document.uri.as_str()) {
                crate::formatting::edits(
                    document.analysis.name_snapshot().sources(),
                    document.source_id,
                )
            } else {
                Ok(None)
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(document) = documents.get(params.text_document.uri.as_str()) else {
                return Ok(None);
            };
            crate::formatting::edits(
                document.analysis.name_snapshot().sources(),
                document.source_id,
            )
        }
    }
    .map_err(RequestError::Internal)
}

fn definition_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<GotoDefinitionResponse>, RequestError> {
    let params: GotoDefinitionParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let position = params.text_document_position_params;
    let locations = match state {
        SessionState::Unit { session, fallback } => {
            if session.contains_uri(&position.text_document.uri) {
                match session.definition_locations(&position.text_document.uri, position.position) {
                    Ok(locations) => locations,
                    Err(UnitDefinitionQueryError::Position(
                        PositionMappingError::InsideSurrogatePair,
                    )) => {
                        return Err(RequestError::InvalidParams(
                            "definition position splits a UTF-16 surrogate pair".to_owned(),
                        ));
                    }
                    Err(error) => return Err(RequestError::Internal(error.to_string())),
                }
            } else if let Some(document) = fallback.get(position.text_document.uri.as_str()) {
                match single_document_definition_locations(
                    document,
                    &position.text_document.uri,
                    position.position,
                )? {
                    Some(locations) => locations,
                    None => return Ok(None),
                }
            } else {
                return Ok(None);
            }
        }
        SessionState::SingleDocument(documents) => {
            let Some(document) = documents.get(position.text_document.uri.as_str()) else {
                return Ok(None);
            };
            match single_document_definition_locations(
                document,
                &position.text_document.uri,
                position.position,
            )? {
                Some(locations) => locations,
                None => return Ok(None),
            }
        }
    };
    Ok(match locations.as_slice() {
        [] => None,
        [location] => Some(GotoDefinitionResponse::Scalar(location.clone())),
        _ => Some(GotoDefinitionResponse::Array(locations)),
    })
}

fn single_document_definition_locations(
    document: &OpenDocument,
    uri: &Uri,
    position: lsp_types::Position,
) -> Result<Option<Vec<Location>>, RequestError> {
    let source_id = document.source_id;
    let sources = document.analysis.name_snapshot().sources();
    let units = document
        .analysis
        .name_snapshot()
        .names()
        .index()
        .source_units();
    let source_unit = units
        .iter()
        .find(|source| source.source_id() == source_id)
        .ok_or_else(|| RequestError::Internal("document source is absent from unit".to_owned()))?
        .id();
    let offset = match byte_offset(sources, source_id, position) {
        Ok(offset) => offset,
        Err(PositionMappingError::InsideSurrogatePair) => {
            return Err(RequestError::InvalidParams(
                "definition position splits a UTF-16 surrogate pair".to_owned(),
            ));
        }
        Err(error) => return Err(RequestError::Internal(error.to_string())),
    };
    let Some(offset) = offset else {
        return Ok(None);
    };
    let locations = document
        .analysis
        .definitions()
        .targets_at(source_unit, offset)
        .iter()
        .map(|target| {
            let target_source = units
                .iter()
                .find(|source| source.id() == target.source_unit)
                .ok_or_else(|| {
                    RequestError::Internal(
                        "definition target source is absent from unit".to_owned(),
                    )
                })?;
            if target_source.source_id() != source_id || target.span.source_id() != source_id {
                return Err(RequestError::Internal(
                    "single document definition target source mismatch".to_owned(),
                ));
            }
            span_range(sources, target.span)
                .map(|range| Location::new(uri.clone(), range))
                .map_err(|error| RequestError::Internal(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(locations))
}

enum RequestError {
    InvalidParams(String),
    Internal(String),
}

fn publish(
    connection: &Connection,
    uri: Uri,
    version: Option<i32>,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), NotificationError> {
    send_notification(
        connection,
        PublishDiagnostics::METHOD,
        serde_json::to_value(PublishDiagnosticsParams::new(uri, diagnostics, version))?,
    )
    .map_err(NotificationError::Server)
}

fn send_log(connection: &Connection, message: String) -> Result<(), ServerError> {
    send_notification(
        connection,
        LogMessage::METHOD,
        serde_json::to_value(LogMessageParams {
            typ: MessageType::ERROR,
            message,
        })?,
    )
}

fn send_notification(
    connection: &Connection,
    method: &'static str,
    params: serde_json::Value,
) -> Result<(), ServerError> {
    let notification = Notification {
        method: method.to_owned(),
        params,
    };
    connection
        .sender
        .send(Message::Notification(notification))
        .map_err(|_| ServerError::Disconnected)
}

fn send_response(connection: &Connection, response: Response) -> Result<(), ServerError> {
    connection
        .sender
        .send(Message::Response(response))
        .map_err(|_| ServerError::Disconnected)
}

/// 会话级 transport 或协议失败。
#[derive(Debug)]
pub(crate) enum ServerError {
    Protocol(lsp_server::ProtocolError),
    Json(serde_json::Error),
    Internal(String),
    Disconnected,
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(error) => write!(formatter, "LSP protocol error: {error}"),
            Self::Json(error) => write!(formatter, "invalid LSP JSON payload: {error}"),
            Self::Internal(message) => write!(formatter, "LSP internal failure: {message}"),
            Self::Disconnected => formatter.write_str("LSP connection disconnected"),
        }
    }
}

impl Error for ServerError {}

impl From<lsp_server::ProtocolError> for ServerError {
    fn from(error: lsp_server::ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

impl From<serde_json::Error> for ServerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug)]
enum NotificationError {
    Json(serde_json::Error),
    Analysis(AnalysisError),
    Mapping(DiagnosticMappingError),
    Unit(UnitSessionError),
    Server(ServerError),
    ExpectedSingleFullChange,
}

impl fmt::Display for NotificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid notification params: {error}"),
            Self::Analysis(error) => write!(formatter, "document analysis failed: {error}"),
            Self::Mapping(error) => write!(formatter, "diagnostic mapping failed: {error}"),
            Self::Unit(error) => write!(formatter, "source-set update failed: {error}"),
            Self::Server(error) => write!(formatter, "diagnostic publication failed: {error}"),
            Self::ExpectedSingleFullChange => {
                formatter.write_str("didChange must contain exactly one full-document change")
            }
        }
    }
}

macro_rules! notification_error_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for NotificationError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}

notification_error_from!(serde_json::Error, Json);
notification_error_from!(AnalysisError, Analysis);
notification_error_from!(DiagnosticMappingError, Mapping);
notification_error_from!(UnitSessionError, Unit);
notification_error_from!(ServerError, Server);

#[cfg(test)]
mod tests;
