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
    unit_session::{UnitPublication, UnitSession, UnitSessionError},
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
    let SessionState::Legacy(documents) = state else {
        return Ok(None);
    };
    let Some(document) = documents.get(position.text_document.uri.as_str()) else {
        return Ok(None);
    };
    let source_id = document.analysis.definitions.source_id();
    let offset = match byte_offset(&document.analysis.sources, source_id, position.position) {
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
        .definitions
        .targets_at(offset)
        .iter()
        .map(|target| {
            span_range(&document.analysis.sources, *target)
                .map(|range| Location::new(position.text_document.uri.clone(), range))
                .map_err(|error| RequestError::Internal(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
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
mod tests {
    use std::{thread, time::Duration};

    use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId};
    use lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        GotoDefinitionResponse, InitializeParams, InitializeResult, InitializedParams,
        LogMessageParams, Position, PublishDiagnosticsParams, TextDocumentContentChangeEvent,
        TextDocumentIdentifier, TextDocumentItem, VersionedTextDocumentIdentifier,
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            LogMessage, Notification as LspNotification, PublishDiagnostics,
        },
        request::{GotoDefinition, Initialize, Request as LspRequest, Shutdown},
    };

    use super::run;

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn source_set_initialization_accepts_valid_options_and_enters_session() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let mut params = serde_json::to_value(InitializeParams::default()).expect("params");
        params["initializationOptions"] = serde_json::json!({
            "unrelated": true,
            "koven": {
                "sibling": "ignored",
                "sourceSet": {
                    "schema": "koven.lsp.source-set",
                    "version": 1,
                    "roots": ["main"],
                    "sources": [{
                        "root": "main",
                        "logicalPath": "app/main.ko",
                        "uri": "file:///not-read/main.ko",
                        "text": "package app"
                    }]
                }
            }
        });
        client
            .sender
            .send(Message::Request(Request {
                id: RequestId::from(39_i32),
                method: Initialize::METHOD.to_owned(),
                params,
            }))
            .expect("initialize request");
        let response = receive_response(&client);
        assert!(response.response_result.is_ok());
        send_notification(
            &client,
            Initialized::METHOD,
            serde_json::to_value(InitializedParams {}).expect("initialized params"),
        );
        let initial = receive_diagnostics(&client);
        assert_eq!(initial.version, None);
        assert!(initial.diagnostics.is_empty());
        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn source_set_initialization_returns_invalid_params_before_session_start() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let mut params = serde_json::to_value(InitializeParams::default()).expect("params");
        params["initializationOptions"] = serde_json::json!({
            "koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": [],
                "sources": []
            }}
        });
        client
            .sender
            .send(Message::Request(Request {
                id: RequestId::from(40_i32),
                method: Initialize::METHOD.to_owned(),
                params,
            }))
            .expect("initialize request");

        let response = receive_response(&client);
        let error = response.response_result.expect_err("invalid params");
        assert_eq!(error.code, ErrorCode::InvalidParams as i32);
        assert!(error.message.contains("roots must not be empty"));
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn memory_session_publishes_versions_clears_close_and_shuts_down() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        initialize(&client);
        let uri: lsp_types::Uri = "file:///session.ko".parse().expect("uri");

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    uri.clone(),
                    "koven".to_owned(),
                    1,
                    "fun valid(): Unit {}".to_owned(),
                ),
            })
            .expect("open params"),
        );
        let opened = receive_diagnostics(&client);
        assert_eq!(opened.version, Some(1));
        assert!(opened.diagnostics.is_empty());

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
                content_changes: Vec::new(),
            })
            .expect("malformed change params"),
        );
        let Message::Notification(log) = client
            .receiver
            .recv_timeout(TIMEOUT)
            .expect("malformed change log")
        else {
            panic!("expected log notification");
        };
        assert_eq!(log.method, lsp_types::notification::LogMessage::METHOD);

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 3),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "fun invalid(): Unit { missing }".to_owned(),
                }],
            })
            .expect("change params"),
        );
        let changed = receive_diagnostics(&client);
        assert_eq!(changed.version, Some(3));
        assert!(changed.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(lsp_types::NumberOrString::String("L0080".to_owned()))
        }));

        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::to_value(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
            })
            .expect("close params"),
        );
        let closed = receive_diagnostics(&client);
        assert_eq!(closed.uri, uri);
        assert_eq!(closed.version, None);
        assert!(closed.diagnostics.is_empty());

        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn unknown_request_gets_method_not_found_and_unopened_change_is_ignored() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        initialize(&client);
        let request_id = RequestId::from(9_i32);
        client
            .sender
            .send(Message::Request(Request {
                id: request_id.clone(),
                method: "koven/unknown".to_owned(),
                params: serde_json::Value::Null,
            }))
            .expect("request");
        let Message::Response(response) = client.receiver.recv_timeout(TIMEOUT).expect("response")
        else {
            panic!("expected response");
        };
        assert_eq!(response.id, request_id);
        assert_eq!(
            response.response_result.expect_err("method error").code,
            ErrorCode::MethodNotFound as i32
        );

        let uri: lsp_types::Uri = "file:///closed.ko".parse().expect("uri");
        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri, 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "@".to_owned(),
                }],
            })
            .expect("change params"),
        );
        send_notification(
            &client,
            "koven/unknownNotification",
            serde_json::Value::Null,
        );
        assert!(
            client
                .receiver
                .recv_timeout(Duration::from_millis(100))
                .is_err()
        );

        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn definition_uses_utf16_latest_version_and_open_document_lifecycle() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        initialize(&client);
        let uri: lsp_types::Uri = "file:///definition.ko".parse().expect("uri");
        let source = "fun target(): Unit {}\nfun use(): Unit { /* 😀 */ target() }";

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    uri.clone(),
                    "koven".to_owned(),
                    1,
                    source.to_owned(),
                ),
            })
            .expect("open params"),
        );
        receive_diagnostics(&client);

        let malformed_id = RequestId::from(20_i32);
        send_request(
            &client,
            malformed_id.clone(),
            GotoDefinition::METHOD,
            serde_json::Value::Null,
        );
        let malformed = receive_response(&client);
        assert_eq!(malformed.id, malformed_id);
        assert_eq!(
            malformed.response_result.expect_err("invalid params").code,
            ErrorCode::InvalidParams as i32
        );

        let first = request_definition(
            &client,
            RequestId::from(21_i32),
            &uri,
            position_of(source, "target", 1),
        )
        .expect("definition response");
        let GotoDefinitionResponse::Scalar(first) = first else {
            panic!("expected one definition");
        };
        assert_eq!(first.uri, uri);
        assert_eq!(first.range.start, Position::new(0, 4));
        assert_eq!(first.range.end, Position::new(0, 10));

        let changed_source = "\nfun target(): Unit {}\nfun use(): Unit { target() }";
        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: changed_source.to_owned(),
                }],
            })
            .expect("change params"),
        );
        assert_eq!(receive_diagnostics(&client).version, Some(2));
        let changed = request_definition(
            &client,
            RequestId::from(22_i32),
            &uri,
            position_of(changed_source, "target", 1),
        )
        .expect("changed definition");
        let GotoDefinitionResponse::Scalar(changed) = changed else {
            panic!("expected one changed definition");
        };
        assert_eq!(changed.range.start, Position::new(1, 4));

        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::to_value(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
            })
            .expect("close params"),
        );
        receive_diagnostics(&client);
        assert!(
            request_definition(&client, RequestId::from(23_i32), &uri, Position::new(1, 4),)
                .is_none()
        );

        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn source_set_lifecycle_rebuilds_all_diagnostics_and_restores_base_text() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let provider_uri: lsp_types::Uri = "file:///workspace/p/api.ko".parse().expect("uri");
        let consumer_uri: lsp_types::Uri = "file:///workspace/q/use.ko".parse().expect("uri");
        initialize_with_options(
            &client,
            Some(serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["main"],
                "sources": [
                    {
                        "root": "main",
                        "logicalPath": "q/use.ko",
                        "uri": consumer_uri,
                        "text": "package q\nfun use(): Int = p.missing()"
                    },
                    {
                        "root": "main",
                        "logicalPath": "p/api.ko",
                        "uri": provider_uri,
                        "text": "package p\nfun make(): Int = 1"
                    }
                ]
            }}})),
        );

        let initial = receive_publications(&client, 2);
        assert_publication_order(&initial, &provider_uri, &consumer_uri);
        assert_eq!(initial[0].version, None);
        assert_eq!(initial[1].version, None);
        assert!(has_code(&initial[1], "L0080"));

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    consumer_uri.clone(),
                    "koven".to_owned(),
                    1,
                    "package q\nfun use(): Int = p.make()".to_owned(),
                ),
            })
            .expect("consumer open"),
        );
        let consumer_open = receive_publications(&client, 2);
        assert_publication_order(&consumer_open, &provider_uri, &consumer_uri);
        assert_eq!(consumer_open[0].version, None);
        assert_eq!(consumer_open[1].version, Some(1));
        assert!(
            consumer_open.iter().all(|item| item.diagnostics.is_empty()),
            "{consumer_open:?}"
        );

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    provider_uri.clone(),
                    "koven".to_owned(),
                    5,
                    "package p\nfun other(): Int = 2".to_owned(),
                ),
            })
            .expect("provider open"),
        );
        let provider_open = receive_publications(&client, 2);
        assert_eq!(provider_open[0].version, Some(5));
        assert_eq!(provider_open[1].version, Some(1));
        assert!(has_code(&provider_open[1], "L0080"));

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(consumer_uri.clone(), 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "package q\nfun use(): Int = p.other()".to_owned(),
                }],
            })
            .expect("consumer change"),
        );
        let consumer_change = receive_publications(&client, 2);
        assert_eq!(consumer_change[0].version, Some(5));
        assert_eq!(consumer_change[1].version, Some(2));
        assert!(
            consumer_change
                .iter()
                .all(|item| item.diagnostics.is_empty())
        );

        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::to_value(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(provider_uri.clone()),
            })
            .expect("provider close"),
        );
        let provider_close = receive_publications(&client, 2);
        assert_eq!(provider_close[0].version, None);
        assert_eq!(provider_close[1].version, Some(2));
        assert!(has_code(&provider_close[1], "L0080"));

        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::to_value(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(consumer_uri.clone()),
            })
            .expect("consumer close"),
        );
        let consumer_close = receive_publications(&client, 2);
        assert_eq!(consumer_close[0].version, None);
        assert_eq!(consumer_close[1].version, None);
        assert!(has_code(&consumer_close[1], "L0080"));

        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn source_set_protocol_events_log_and_preserve_last_good_state() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let uri: lsp_types::Uri = "file:///workspace/app/main.ko".parse().expect("uri");
        let unknown: lsp_types::Uri = "file:///workspace/app/unknown.ko".parse().expect("uri");
        initialize_with_options(
            &client,
            Some(serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["main"],
                "sources": [{
                    "root": "main",
                    "logicalPath": "app/main.ko",
                    "uri": uri,
                    "text": "package app\nfun main(): Unit {}"
                }]
            }}})),
        );
        receive_diagnostics(&client);

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    unknown.clone(),
                    "koven".to_owned(),
                    1,
                    "package app".to_owned(),
                ),
            })
            .expect("unknown open"),
        );
        assert!(receive_log(&client).message.contains("unknown URI"));

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    uri.clone(),
                    "koven".to_owned(),
                    4,
                    "package app\nfun main(): Unit {}".to_owned(),
                ),
            })
            .expect("known open"),
        );
        assert_eq!(receive_diagnostics(&client).version, Some(4));

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    uri.clone(),
                    "koven".to_owned(),
                    5,
                    "@".to_owned(),
                ),
            })
            .expect("duplicate open"),
        );
        assert!(receive_log(&client).message.contains("duplicated URI"));

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 4),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "@".to_owned(),
                }],
            })
            .expect("stale change"),
        );
        assert!(receive_log(&client).message.contains("is not newer"));

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 5),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: Some(lsp_types::Range::new(
                        Position::new(0, 0),
                        Position::new(0, 0),
                    )),
                    range_length: None,
                    text: "@".to_owned(),
                }],
            })
            .expect("partial change"),
        );
        assert!(receive_log(&client).message.contains("full-document"));

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 5),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "package app\nfun changed(): Unit {}".to_owned(),
                }],
            })
            .expect("valid change"),
        );
        let changed = receive_diagnostics(&client);
        assert_eq!(changed.version, Some(5));
        assert!(changed.diagnostics.is_empty());

        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::to_value(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
            })
            .expect("close"),
        );
        assert_eq!(receive_diagnostics(&client).version, None);

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 6),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "@".to_owned(),
                }],
            })
            .expect("unopened change"),
        );
        assert!(receive_log(&client).message.contains("unopened URI"));

        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::to_value(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(unknown),
            })
            .expect("unknown close"),
        );
        assert!(receive_log(&client).message.contains("unopened URI"));

        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn source_set_internal_analysis_failure_preserves_last_good_snapshot() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let uri: lsp_types::Uri = "file:///workspace/app/main.ko".parse().expect("uri");
        initialize_with_options(
            &client,
            Some(serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["main"],
                "sources": [{
                    "root": "main",
                    "logicalPath": "app/main.ko",
                    "uri": uri,
                    "text": "package app\nfun main(): Unit {}"
                }]
            }}})),
        );
        receive_diagnostics(&client);
        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    uri.clone(),
                    "koven".to_owned(),
                    1,
                    "package app\nfun main(): Unit {}".to_owned(),
                ),
            })
            .expect("open"),
        );
        assert_eq!(receive_diagnostics(&client).version, Some(1));

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "package app\nfun broken(): Unit { this }".to_owned(),
                }],
            })
            .expect("internally unsupported change"),
        );
        assert!(
            receive_log(&client)
                .message
                .contains("does not yet support node")
        );

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "package app\nfun recovered(): Unit {}".to_owned(),
                }],
            })
            .expect("recovery change"),
        );
        let recovered = receive_diagnostics(&client);
        assert_eq!(recovered.version, Some(2));
        assert!(recovered.diagnostics.is_empty());

        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    #[test]
    fn source_set_diagnostics_follow_parser_type_and_ownership_validation_gates() {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let provider_uri: lsp_types::Uri = "file:///workspace/p/api.ko".parse().expect("uri");
        let consumer_uri: lsp_types::Uri = "file:///workspace/q/use.ko".parse().expect("uri");
        initialize_with_options(
            &client,
            Some(serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["main"],
                "sources": [
                    {
                        "root": "main",
                        "logicalPath": "p/api.ko",
                        "uri": provider_uri,
                        "text": "package p\nfun helper(): Unit {}"
                    },
                    {
                        "root": "main",
                        "logicalPath": "q/use.ko",
                        "uri": consumer_uri,
                        "text": "package q\nfun ok(): Unit {}"
                    }
                ]
            }}})),
        );
        receive_publications(&client, 2);

        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    consumer_uri.clone(),
                    "koven".to_owned(),
                    1,
                    "#".to_owned(),
                ),
            })
            .expect("parser overlay"),
        );
        let parser = receive_publications(&client, 2);
        assert_publication_order(&parser, &provider_uri, &consumer_uri);
        assert!(parser[0].diagnostics.is_empty());
        assert_eq!(code_count(&parser[1], "L0001"), 1);
        assert_eq!(code_count(&parser[1], "L0084"), 0);
        assert_eq!(code_count(&parser[1], "L0131"), 0);

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(consumer_uri.clone(), 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "package q\nfun typed(): Unit { val item: String = 1 }".to_owned(),
                }],
            })
            .expect("type overlay"),
        );
        let typed = receive_publications(&client, 2);
        assert!(typed[0].diagnostics.is_empty());
        assert_eq!(code_count(&typed[1], "L0001"), 0);
        assert_eq!(code_count(&typed[1], "L0084"), 1);
        assert_eq!(code_count(&typed[1], "L0131"), 0);

        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(consumer_uri.clone(), 3),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "package q\n\
                           class Resource()\n\
                           fun take(own resource: Resource): Unit {}\n\
                           fun moved(own resource: Resource): Unit {\n\
                               val first = take(resource)\n\
                               val second = take(resource)\n\
                           }"
                    .to_owned(),
                }],
            })
            .expect("ownership overlay"),
        );
        let owned = receive_publications(&client, 2);
        assert!(owned[0].diagnostics.is_empty());
        assert_eq!(code_count(&owned[1], "L0001"), 0);
        assert_eq!(code_count(&owned[1], "L0084"), 0);
        assert_eq!(code_count(&owned[1], "L0131"), 1);

        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    fn initialize(client: &Connection) {
        initialize_with_options(client, None);
    }

    fn initialize_with_options(client: &Connection, options: Option<serde_json::Value>) {
        let mut params = serde_json::to_value(InitializeParams::default()).expect("params");
        if let Some(options) = options {
            params["initializationOptions"] = options;
        }
        client
            .sender
            .send(Message::Request(Request {
                id: RequestId::from(1_i32),
                method: Initialize::METHOD.to_owned(),
                params,
            }))
            .expect("initialize request");
        let Message::Response(response) = client.receiver.recv_timeout(TIMEOUT).expect("response")
        else {
            panic!("expected initialize response");
        };
        let result: InitializeResult =
            serde_json::from_value(response.response_result.expect("initialize result"))
                .expect("typed initialize result");
        assert_eq!(
            result.capabilities.position_encoding,
            Some(lsp_types::PositionEncodingKind::UTF16)
        );
        let sync = result
            .capabilities
            .text_document_sync
            .expect("sync capability");
        assert!(matches!(
            sync,
            lsp_types::TextDocumentSyncCapability::Options(options)
                if options.open_close == Some(true)
                    && options.change == Some(lsp_types::TextDocumentSyncKind::FULL)
        ));
        assert_eq!(
            result.capabilities.definition_provider,
            Some(lsp_types::OneOf::Left(true))
        );
        send_notification(
            client,
            Initialized::METHOD,
            serde_json::to_value(InitializedParams {}).expect("initialized params"),
        );
    }

    fn shutdown(client: &Connection) {
        client
            .sender
            .send(Message::Request(Request {
                id: RequestId::from(2_i32),
                method: Shutdown::METHOD.to_owned(),
                params: serde_json::Value::Null,
            }))
            .expect("shutdown request");
        let Message::Response(_) = client
            .receiver
            .recv_timeout(TIMEOUT)
            .expect("shutdown response")
        else {
            panic!("expected shutdown response");
        };
        send_notification(
            client,
            Exit::METHOD,
            serde_json::to_value(()).expect("exit params"),
        );
    }

    fn send_notification(client: &Connection, method: &'static str, params: serde_json::Value) {
        client
            .sender
            .send(Message::Notification(Notification {
                method: method.to_owned(),
                params,
            }))
            .expect("notification");
    }

    fn request_definition(
        client: &Connection,
        id: RequestId,
        uri: &lsp_types::Uri,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        send_request(
            client,
            id.clone(),
            GotoDefinition::METHOD,
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": position,
            }),
        );
        let response = receive_response(client);
        assert_eq!(response.id, id);
        serde_json::from_value(response.response_result.expect("definition result"))
            .expect("typed definition response")
    }

    fn send_request(
        client: &Connection,
        id: RequestId,
        method: &'static str,
        params: serde_json::Value,
    ) {
        client
            .sender
            .send(Message::Request(Request {
                id,
                method: method.to_owned(),
                params,
            }))
            .expect("request");
    }

    fn receive_response(client: &Connection) -> lsp_server::Response {
        let Message::Response(response) = client.receiver.recv_timeout(TIMEOUT).expect("response")
        else {
            panic!("expected response");
        };
        response
    }

    fn position_of(source: &str, needle: &str, index: usize) -> Position {
        let offset = source
            .match_indices(needle)
            .nth(index)
            .map(|(offset, _)| offset)
            .expect("needle occurrence");
        let prefix = &source[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = prefix.rfind('\n').map_or(0, |newline| newline + 1);
        let character = source[line_start..offset].encode_utf16().count();
        Position::new(
            u32::try_from(line).expect("line"),
            u32::try_from(character).expect("character"),
        )
    }

    fn receive_diagnostics(client: &Connection) -> PublishDiagnosticsParams {
        let Message::Notification(notification) = client
            .receiver
            .recv_timeout(TIMEOUT)
            .expect("diagnostics notification")
        else {
            panic!("expected diagnostics notification");
        };
        assert_eq!(notification.method, PublishDiagnostics::METHOD);
        serde_json::from_value(notification.params).expect("diagnostics params")
    }

    fn receive_publications(client: &Connection, count: usize) -> Vec<PublishDiagnosticsParams> {
        (0..count).map(|_| receive_diagnostics(client)).collect()
    }

    fn receive_log(client: &Connection) -> LogMessageParams {
        let Message::Notification(notification) = client
            .receiver
            .recv_timeout(TIMEOUT)
            .expect("log notification")
        else {
            panic!("expected log notification");
        };
        assert_eq!(notification.method, LogMessage::METHOD);
        serde_json::from_value(notification.params).expect("log params")
    }

    fn assert_publication_order(
        publications: &[PublishDiagnosticsParams],
        first: &lsp_types::Uri,
        second: &lsp_types::Uri,
    ) {
        assert_eq!(publications.len(), 2);
        assert_eq!(&publications[0].uri, first);
        assert_eq!(&publications[1].uri, second);
    }

    fn has_code(publication: &PublishDiagnosticsParams, code: &str) -> bool {
        code_count(publication, code) != 0
    }

    fn code_count(publication: &PublishDiagnosticsParams, code: &str) -> usize {
        publication
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == Some(lsp_types::NumberOrString::String(code.to_owned()))
            })
            .count()
    }
}
