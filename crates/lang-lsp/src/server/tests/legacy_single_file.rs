//! SPEC-0253 旧 legacy 协议合同，不套用 source-set 的版本/事务策略。

use std::collections::BTreeMap;

use lsp_types::{Location, Range, Uri};

use super::*;
use crate::{
    diagnostic_adapter::convert_diagnostics,
    server::{NotificationError, OpenDocument, ServerError, handle_legacy_notification},
    single_file_oracle::{RECOVERY, assert_diagnostics, manual, position, target_snapshot},
};

#[test]
fn legacy_protocol_recovery_equal_older_and_duplicate_open_match_manual_publications() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize(&client);
    let uri: Uri = "file:///legacy-recovery.ko".parse().expect("URI");
    // 与 unit session 不同：同版本、旧版本 change 和重复 open 均被旧 legacy 接受。
    for (index, method, version, prefix) in [
        (0, DidOpenTextDocument::METHOD, 5, ""),
        (1, DidChangeTextDocument::METHOD, 5, "\r\n"),
        (2, DidChangeTextDocument::METHOD, 3, "\r\n\r\n"),
        (3, DidOpenTextDocument::METHOD, 2, "\r\n\r\n\r\n"),
    ] {
        let source = format!("{prefix}{RECOVERY}");
        let notification = if method == DidOpenTextDocument::METHOD {
            open(&uri, version, &source)
        } else {
            change(&uri, version, &source)
        };
        send_notification(&client, method, notification.params);
        assert_eq!(
            receive_diagnostics(&client),
            publication(&uri, version, &source)
        );
        let declaration = source.find("target").expect("declaration");
        let reference = source.rfind("target").expect("reference");
        assert_eq!(
            request_definition(
                &client,
                RequestId::from(100 + index),
                &uri,
                position(&source, reference)
            ),
            Some(GotoDefinitionResponse::Scalar(Location::new(
                uri.clone(),
                Range::new(
                    position(&source, declaration),
                    position(&source, declaration + 6)
                )
            )))
        );
        // 真实协议必须拒绝 surrogate 中点，而非把 UTF-16 column 当 byte offset。
        let emoji = source.rfind('😀').expect("astral scalar");
        let mut split = position(&source, emoji);
        split.character += 1;
        send_request(
            &client,
            RequestId::from(200 + index),
            GotoDefinition::METHOD,
            serde_json::json!({"textDocument": {"uri": uri}, "position": split}),
        );
        assert_eq!(
            receive_response(&client)
                .response_result
                .expect_err("invalid surrogate position")
                .code,
            ErrorCode::InvalidParams as i32
        );
    }
    for _ in 0..2 {
        let notification = close(&uri);
        send_notification(&client, DidCloseTextDocument::METHOD, notification.params);
        assert_eq!(
            receive_diagnostics(&client),
            PublishDiagnosticsParams::new(uri.clone(), Vec::new(), None)
        );
    }
    assert!(request_definition(&client, RequestId::from(300), &uri, Position::new(0, 0)).is_none());
    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn legacy_invalid_changes_preserve_version_diagnostics_and_navigation_without_publication() {
    let (server, client) = Connection::memory();
    let uri: Uri = "file:///legacy-atomic.ko".parse().expect("URI");
    let mut documents = BTreeMap::new();
    handle_legacy_notification(&server, open(&uri, 7, RECOVERY), &mut documents)
        .expect("initial open");
    assert_eq!(receive_diagnostics(&client), publication(&uri, 7, RECOVERY));
    for changes in [
        serde_json::json!([]),
        serde_json::json!([{"text": ""}, {"text": ""}]),
        serde_json::json!([{"text": "", "range": {"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}]),
        serde_json::json!([{"text": "", "rangeLength": 0}]),
    ] {
        let notification = Notification::new(
            DidChangeTextDocument::METHOD.to_owned(),
            serde_json::json!({"textDocument":{"uri":uri,"version":8}, "contentChanges":changes}),
        );
        assert!(matches!(
            handle_legacy_notification(&server, notification, &mut documents),
            Err(NotificationError::ExpectedSingleFullChange)
        ));
        assert_document(&documents, &uri, 7, RECOVERY);
        assert!(
            client.receiver.try_recv().is_err(),
            "invalid change published"
        );
    }
    // 旧链先判 unopened，再检查 full-change shape；空 changes 在 unopened URI 上被忽略。
    let unopened: Uri = "file:///never-opened.ko".parse().expect("URI");
    let ignored = Notification::new(
        DidChangeTextDocument::METHOD.to_owned(),
        serde_json::json!({"textDocument":{"uri":unopened,"version":1}, "contentChanges":[]}),
    );
    handle_legacy_notification(&server, ignored, &mut documents).expect("unopened change ignored");
    assert_document(&documents, &uri, 7, RECOVERY);
    assert!(client.receiver.try_recv().is_err());
}

#[test]
fn legacy_open_and_change_publish_before_commit_but_close_removes_before_publish() {
    let uri: Uri = "file:///legacy-disconnected.ko".parse().expect("URI");
    for action in ["open", "duplicate-open", "change", "close"] {
        let (server, client) = Connection::memory();
        let mut documents = BTreeMap::new();
        if action != "open" {
            handle_legacy_notification(&server, open(&uri, 7, RECOVERY), &mut documents)
                .expect("initial commit");
            assert_eq!(receive_diagnostics(&client), publication(&uri, 7, RECOVERY));
        }
        drop(client.receiver);
        let replacement = "fun replacement(): Unit {}";
        let notification = match action {
            "open" | "duplicate-open" => open(&uri, 8, replacement),
            "change" => change(&uri, 8, replacement),
            "close" => close(&uri),
            _ => unreachable!("fixed action fixture"),
        };
        assert!(
            matches!(
                handle_legacy_notification(&server, notification, &mut documents),
                Err(NotificationError::Server(ServerError::Disconnected))
            ),
            "{action}"
        );
        if matches!(action, "open" | "close") {
            assert!(
                documents.is_empty(),
                "{action}: old legacy transaction boundary"
            );
        } else {
            assert_document(&documents, &uri, 7, RECOVERY);
        }
    }
}

fn assert_document(
    documents: &BTreeMap<String, OpenDocument>,
    uri: &Uri,
    version: i32,
    source: &str,
) {
    let document = documents.get(uri.as_str()).expect("committed document");
    assert_eq!(document.version, version);
    let expected = manual(uri.as_str(), source);
    assert_diagnostics(
        &document.analysis.sources,
        &document.analysis.diagnostics,
        &expected.sources,
        &expected.diagnostics,
        uri.as_str(),
    );
    assert_eq!(
        document
            .analysis
            .sources
            .source_text(document.analysis.definitions.source_id())
            .expect("source"),
        source
    );
    for offset in (0..=source.len()).filter(|offset| source.is_char_boundary(*offset)) {
        assert_eq!(
            target_snapshot(
                &document.analysis.sources,
                document.analysis.definitions.targets_at(offset)
            ),
            target_snapshot(&expected.sources, expected.definitions.targets_at(offset)),
            "navigation retained at {offset}"
        );
    }
}

fn publication(uri: &Uri, version: i32, source: &str) -> PublishDiagnosticsParams {
    let expected = manual(uri.as_str(), source);
    PublishDiagnosticsParams::new(
        uri.clone(),
        convert_diagnostics(&expected.sources, uri, &expected.diagnostics)
            .expect("manual publication"),
        Some(version),
    )
}

fn open(uri: &Uri, version: i32, text: &str) -> Notification {
    Notification::new(
        DidOpenTextDocument::METHOD.to_owned(),
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri.clone(),
                "koven".to_owned(),
                version,
                text.to_owned(),
            ),
        },
    )
}

fn change(uri: &Uri, version: i32, text: &str) -> Notification {
    Notification::new(
        DidChangeTextDocument::METHOD.to_owned(),
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_owned(),
            }],
        },
    )
}

fn close(uri: &Uri) -> Notification {
    Notification::new(
        DidCloseTextDocument::METHOD.to_owned(),
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
        },
    )
}
