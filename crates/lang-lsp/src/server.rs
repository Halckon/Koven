//! LSP lifecycle、打开文档状态与诊断发布。

use std::{collections::BTreeMap, error::Error, fmt};

use lsp_server::{Connection, ErrorCode, Message, Notification, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, LogMessageParams, MessageType, PositionEncodingKind,
    PublishDiagnosticsParams, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, Uri,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, LogMessage,
        Notification as LspNotification, PublishDiagnostics,
    },
};

use crate::{
    analysis::{AnalysisError, analyze},
    diagnostic_adapter::{DiagnosticMappingError, convert_diagnostics},
};

/// 运行一个已建立 transport 的 Koven LSP 会话。
pub(crate) fn run(connection: Connection) -> Result<(), ServerError> {
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let _: InitializeParams = serde_json::from_value(initialize_params)?;
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
        ..ServerCapabilities::default()
    }
}

struct OpenDocument {
    version: i32,
    text: String,
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
            let document = OpenDocument {
                version: params.text_document.version,
                text: params.text_document.text,
            };
            publish_document(connection, &uri, &document)?;
            documents.insert(uri.as_str().to_owned(), document);
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            let Some(document) = documents.get_mut(params.text_document.uri.as_str()) else {
                return Ok(());
            };
            let [change] = params.content_changes.as_slice() else {
                return Err(NotificationError::ExpectedSingleFullChange);
            };
            if change.range.is_some() || change.range_length.is_some() {
                return Err(NotificationError::ExpectedSingleFullChange);
            }
            document.version = params.text_document.version;
            document.text.clone_from(&change.text);
            publish_document(connection, &params.text_document.uri, document)?;
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
    let analysis = analyze(uri.as_str(), &document.text)?;
    let diagnostics = convert_diagnostics(&analysis.sources, uri, &analysis.diagnostics)?;
    publish(connection, uri.clone(), Some(document.version), diagnostics)
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
        InitializeParams, InitializeResult, InitializedParams, PublishDiagnosticsParams,
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        VersionedTextDocumentIdentifier,
        notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized,
            Notification as LspNotification, PublishDiagnostics,
        },
        request::{Initialize, Request as LspRequest, Shutdown},
    };

    use super::run;

    const TIMEOUT: Duration = Duration::from_secs(5);

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
