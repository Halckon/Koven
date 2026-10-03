// 所有测试与共享 helper 仅通过 server.rs 的 cfg(test) 入口编译。
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

mod definition;
mod initialization;
mod lifecycle;
mod snapshot_lifecycle;
mod source_set;

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

#[allow(clippy::too_many_arguments)]
fn assert_scalar_definition(
    client: &Connection,
    id: RequestId,
    source_uri: &lsp_types::Uri,
    source_position: Position,
    target_uri: &lsp_types::Uri,
    target_source: &str,
    target_needle: &str,
    target_index: usize,
) {
    let definition =
        request_definition(client, id, source_uri, source_position).expect("definition response");
    let GotoDefinitionResponse::Scalar(location) = definition else {
        panic!("expected one definition");
    };
    let start = position_of(target_source, target_needle, target_index);
    assert_eq!(&location.uri, target_uri);
    assert_eq!(location.range.start, start);
    assert_eq!(
        location.range.end,
        Position::new(
            start.line,
            start.character
                + u32::try_from(target_needle.encode_utf16().count()).expect("target width"),
        )
    );
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

mod legacy_single_file;
