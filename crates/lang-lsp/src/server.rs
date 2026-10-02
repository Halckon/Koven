//! LSP lifecycle、打开文档状态与诊断发布。

use std::{collections::BTreeMap, error::Error, fmt};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    GotoDefinitionParams, GotoDefinitionResponse, InitializeParams, InitializeResult, Location,
    LogMessageParams, MessageType, OneOf, PositionEncodingKind, PublishDiagnosticsParams,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, Uri,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, LogMessage,
        Notification as LspNotification, PublishDiagnostics,
    },
    request::{GotoDefinition, Request as LspRequest},
};

use crate::{
    analysis::{Analysis, AnalysisError, analyze},
    diagnostic_adapter::{DiagnosticMappingError, convert_diagnostics},
    position_adapter::{PositionMappingError, byte_offset, span_range},
    source_set::SourceSetConfig,
    unit_session::{UnitDefinitionQueryError, UnitPublication, UnitSession, UnitSessionError},
};

/// 运行一个已建立 transport 的 Koven LSP 会话。
pub(crate) fn run(connection: Connection) -> Result<(), ServerError> {
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
    let (mut state, initial_publications) = match source_set {
        Some(config) => match UnitSession::new(config) {
            Ok(session) => match session.publications() {
                Ok(publications) => (SessionState::Unit(Box::new(session)), Some(publications)),
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
        None => (SessionState::Legacy(BTreeMap::new()), None),
    };
    let initialize_result = InitializeResult {
        capabilities: capabilities(),
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
                if request.method == GotoDefinition::METHOD {
                    handle_definition_request(&connection, request, &state)?;
                    continue;
                }
                send_response(
                    &connection,
                    Response::new_err(
                        request.id,
                        ErrorCode::MethodNotFound as i32,
                        format!("unsupported request method {:?}", request.method),
                    ),
                )?;
            }
            Message::Notification(notification) => {
                if let Err(error) = handle_notification(&connection, notification, &mut state) {
                    send_log(&connection, error.to_string())?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(PositionEncodingKind::UTF16),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                will_save: None,
                will_save_wait_until: None,
                save: None,
            },
        )),
        definition_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    }
}

struct OpenDocument {
    version: i32,
    analysis: Analysis,
}

enum SessionState {
    Legacy(BTreeMap<String, OpenDocument>),
    Unit(Box<UnitSession>),
}

impl OpenDocument {
    fn analyze(uri: &Uri, version: i32, text: &str) -> Result<Self, AnalysisError> {
        Ok(Self {
            version,
            analysis: analyze(uri.as_str(), text)?,
        })
    }
}

fn handle_notification(
    connection: &Connection,
    notification: Notification,
    state: &mut SessionState,
) -> Result<(), NotificationError> {
    match state {
        SessionState::Legacy(documents) => {
            handle_legacy_notification(connection, notification, documents)
        }
        SessionState::Unit(session) => handle_unit_notification(connection, notification, session),
    }
}

fn handle_legacy_notification(
    connection: &Connection,
    notification: Notification,
    documents: &mut BTreeMap<String, OpenDocument>,
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
            publish_document(connection, &uri, &document)?;
            documents.insert(uri.as_str().to_owned(), document);
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            let Some(_) = documents.get(params.text_document.uri.as_str()) else {
                return Ok(());
            };
            let [change] = params.content_changes.as_slice() else {
                return Err(NotificationError::ExpectedSingleFullChange);
            };
            if change.range.is_some() || change.range_length.is_some() {
                return Err(NotificationError::ExpectedSingleFullChange);
            }
            let document = OpenDocument::analyze(
                &params.text_document.uri,
                params.text_document.version,
                &change.text,
            )?;
            publish_document(connection, &params.text_document.uri, &document)?;
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

fn handle_unit_notification(
    connection: &Connection,
    notification: Notification,
    session: &mut UnitSession,
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
            let [change] = params.content_changes.as_slice() else {
                return Err(NotificationError::ExpectedSingleFullChange);
            };
            if change.range.is_some() || change.range_length.is_some() {
                return Err(NotificationError::ExpectedSingleFullChange);
            }
            session.prepare_change(
                &params.text_document.uri,
                params.text_document.version,
                change.text.clone(),
            )?
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
) -> Result<(), NotificationError> {
    let diagnostics = convert_diagnostics(
        &document.analysis.sources,
        uri,
        &document.analysis.diagnostics,
    )?;
    publish(connection, uri.clone(), Some(document.version), diagnostics)
}

fn handle_definition_request(
    connection: &Connection,
    request: Request,
    state: &SessionState,
) -> Result<(), ServerError> {
    let response = match definition_response(request.params, state) {
        Ok(result) => Response::new_ok(request.id, serde_json::to_value(result)?),
        Err(RequestError::InvalidParams(message)) => {
            Response::new_err(request.id, ErrorCode::InvalidParams as i32, message)
        }
        Err(RequestError::Internal(message)) => {
            Response::new_err(request.id, ErrorCode::InternalError as i32, message)
        }
    };
    send_response(connection, response)
}

fn definition_response(
    params: serde_json::Value,
    state: &SessionState,
) -> Result<Option<GotoDefinitionResponse>, RequestError> {
    let params: GotoDefinitionParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let position = params.text_document_position_params;
    let locations = match state {
        SessionState::Unit(session) => {
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
        }
        SessionState::Legacy(documents) => {
            let Some(document) = documents.get(position.text_document.uri.as_str()) else {
                return Ok(None);
            };
            let source_id = document.analysis.definitions.source_id();
            let offset = match byte_offset(&document.analysis.sources, source_id, position.position)
            {
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
            document
                .analysis
                .definitions
                .targets_at(offset)
                .iter()
                .map(|target| {
                    span_range(&document.analysis.sources, *target)
                        .map(|range| Location::new(position.text_document.uri.clone(), range))
                        .map_err(|error| RequestError::Internal(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    Ok(match locations.as_slice() {
        [] => None,
        [location] => Some(GotoDefinitionResponse::Scalar(location.clone())),
        _ => Some(GotoDefinitionResponse::Array(locations)),
    })
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
