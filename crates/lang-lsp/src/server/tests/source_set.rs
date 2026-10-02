use super::*;

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
    let stable = "package app\nfun target(): Unit {}\nfun use(): Unit { target() }";
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
                "text": stable
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
                stable.to_owned(),
            ),
        })
        .expect("open"),
    );
    assert_eq!(receive_diagnostics(&client).version, Some(1));
    assert_scalar_definition(
        &client,
        RequestId::from(82_i32),
        &uri,
        position_of(stable, "target", 1),
        &uri,
        stable,
        "target",
        0,
    );

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
    assert_scalar_definition(
        &client,
        RequestId::from(83_i32),
        &uri,
        position_of(stable, "target", 1),
        &uri,
        stable,
        "target",
        0,
    );

    let recovered_source = "package app\n\nfun target(): Unit {}\nfun use(): Unit { target() }";
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: recovered_source.to_owned(),
            }],
        })
        .expect("recovery change"),
    );
    let recovered = receive_diagnostics(&client);
    assert_eq!(recovered.version, Some(2));
    assert!(recovered.diagnostics.is_empty());
    assert_scalar_definition(
        &client,
        RequestId::from(84_i32),
        &uri,
        position_of(recovered_source, "target", 1),
        &uri,
        recovered_source,
        "target",
        0,
    );

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
