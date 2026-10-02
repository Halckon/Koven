use super::*;

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
