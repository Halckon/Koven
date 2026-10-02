use super::*;

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
        request_definition(&client, RequestId::from(23_i32), &uri, Position::new(1, 4),).is_none()
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn source_set_definition_resolves_imports_qualified_and_same_package() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let provider_uri: lsp_types::Uri = "file:///workspace/lib/api.ko".parse().expect("uri");
    let same_uri: lsp_types::Uri = "file:///workspace/lib/use.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///workspace/app/use.ko".parse().expect("uri");
    let provider = "package lib\n/* 😀 */ public fun exactValue(): Int = 1";
    let same = "package lib\nfun same(): Int = exactValue()";
    let app = "package app\nimport lib.exactValue as Alias\nimport lib.*\n\
        fun exact(): Int = /* 😀 */ Alias()\n\
        fun wild(): Int = exactValue()\n\
        fun qualified(): Int = lib.exactValue()";
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["main"],
            "sources": [
                {
                    "root": "main",
                    "logicalPath": "lib/api.ko",
                    "uri": provider_uri,
                    "text": provider
                },
                {
                    "root": "main",
                    "logicalPath": "lib/use.ko",
                    "uri": same_uri,
                    "text": same
                },
                {
                    "root": "main",
                    "logicalPath": "app/use.ko",
                    "uri": app_uri,
                    "text": app
                }
            ]
        }}})),
    );
    let initial = receive_publications(&client, 3);
    assert!(
        initial
            .iter()
            .all(|publication| publication.diagnostics.is_empty()),
        "{initial:#?}"
    );

    for (id, needle, index) in [
        (70, "exactValue", 0),
        (71, "Alias", 0),
        (72, "Alias", 1),
        (73, "exactValue", 1),
        (74, "exactValue", 2),
    ] {
        assert_scalar_definition(
            &client,
            RequestId::from(id),
            &app_uri,
            position_of(app, needle, index),
            &provider_uri,
            provider,
            "exactValue",
            0,
        );
    }
    assert_scalar_definition(
        &client,
        RequestId::from(75_i32),
        &same_uri,
        position_of(same, "exactValue", 0),
        &provider_uri,
        provider,
        "exactValue",
        0,
    );
    assert!(
        request_definition(
            &client,
            RequestId::from(76_i32),
            &app_uri,
            position_of(app, "*", 0),
        )
        .is_none()
    );
    assert!(
        request_definition(
            &client,
            RequestId::from(77_i32),
            &app_uri,
            position_of(app, "lib", 2),
        )
        .is_none()
    );
    let unknown_uri: lsp_types::Uri = "file:///workspace/unknown.ko".parse().expect("uri");
    assert!(
        request_definition(
            &client,
            RequestId::from(78_i32),
            &unknown_uri,
            Position::new(0, 0),
        )
        .is_none()
    );
    let emoji = position_of(app, "😀", 0);
    let surrogate_id = RequestId::from(79_i32);
    send_request(
        &client,
        surrogate_id.clone(),
        GotoDefinition::METHOD,
        serde_json::json!({
            "textDocument": { "uri": app_uri },
            "position": { "line": emoji.line, "character": emoji.character + 1 },
        }),
    );
    let surrogate = receive_response(&client);
    assert_eq!(surrogate.id, surrogate_id);
    assert_eq!(
        surrogate
            .response_result
            .expect_err("surrogate position must be invalid")
            .code,
        ErrorCode::InvalidParams as i32
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn source_set_definition_rejects_private_and_unresolved_targets() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let provider_uri: lsp_types::Uri = "file:///workspace/lib/private.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///workspace/app/use.ko".parse().expect("uri");
    let app = "package app\nimport lib.secret\nfun hidden(): Int = lib.secret\n\
        fun missing(): Int = absent";
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["main"],
            "sources": [
                {
                    "root": "main",
                    "logicalPath": "lib/private.ko",
                    "uri": provider_uri,
                    "text": "package lib\nprivate val secret = 1"
                },
                {
                    "root": "main",
                    "logicalPath": "app/use.ko",
                    "uri": app_uri,
                    "text": app
                }
            ]
        }}})),
    );
    assert!(
        receive_publications(&client, 2)
            .iter()
            .any(|publication| !publication.diagnostics.is_empty())
    );

    for (id, needle, index) in [(79, "secret", 0), (80, "secret", 1), (81, "absent", 0)] {
        assert!(
            request_definition(
                &client,
                RequestId::from(id),
                &app_uri,
                position_of(app, needle, index),
            )
            .is_none()
        );
    }

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}
