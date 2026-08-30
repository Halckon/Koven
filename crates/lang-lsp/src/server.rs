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
    let _source_set = match SourceSetConfig::from_initialization_options(
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
    let initialize_result = InitializeResult {
        capabilities: capabilities(),
        server_info: Some(ServerInfo {
            name: "koven-lsp".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(initialize_result)?)?;

    let mut documents = BTreeMap::new();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                if request.method == GotoDefinition::METHOD {
                    handle_definition_request(&connection, request, &documents)?;
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
                if let Err(error) = handle_notification(&connection, notification, &mut documents) {
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
    documents: &BTreeMap<String, OpenDocument>,
) -> Result<(), ServerError> {
    let response = match definition_response(request.params, documents) {
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
    documents: &BTreeMap<String, OpenDocument>,
) -> Result<Option<GotoDefinitionResponse>, RequestError> {
    let params: GotoDefinitionParams = serde_json::from_value(params)
        .map_err(|error| RequestError::InvalidParams(error.to_string()))?;
    let position = params.text_document_position_params;
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
    Disconnected,
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(error) => write!(formatter, "LSP protocol error: {error}"),
            Self::Json(error) => write!(formatter, "invalid LSP JSON payload: {error}"),
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
    Server(ServerError),
    ExpectedSingleFullChange,
}

impl fmt::Display for NotificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid notification params: {error}"),
            Self::Analysis(error) => write!(formatter, "document analysis failed: {error}"),
            Self::Mapping(error) => write!(formatter, "diagnostic mapping failed: {error}"),
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
notification_error_from!(ServerError, Server);

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId};
    use lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        GotoDefinitionResponse, InitializeParams, InitializeResult, InitializedParams, Position,
        PublishDiagnosticsParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
        TextDocumentItem, VersionedTextDocumentIdentifier,
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            Notification as LspNotification, PublishDiagnostics,
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

    fn initialize(client: &Connection) {
        client
            .sender
            .send(Message::Request(Request {
                id: RequestId::from(1_i32),
                method: Initialize::METHOD.to_owned(),
                params: serde_json::to_value(InitializeParams::default()).expect("params"),
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
}
