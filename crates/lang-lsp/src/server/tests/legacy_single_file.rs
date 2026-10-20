//! 单文档 Unit 协议保留既有 lifecycle 合同；期望仅来自已审阅的固定观测。

use std::collections::BTreeMap;

use lsp_types::{Location, Range, Uri};

use super::*;
use crate::{
    server::{NotificationError, OpenDocument, ServerError, handle_single_document_notification},
    single_file_oracle::{RECOVERY, assert_recovery_snapshot, position, recovery_publication},
};

#[test]
fn single_document_protocol_recovery_equal_older_and_duplicate_open_match_frozen_publications() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize(&client);
    let uri: Uri = "file:///legacy-recovery.ko".parse().expect("URI");
    // 与 unit session 不同：同版本、旧版本 change 和重复 open 仍被独立单文档会话接受。
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
            recovery_publication(&uri, version, &source)
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
    handle_single_document_notification(&server, open(&uri, 7, RECOVERY), &mut documents)
        .expect("initial open");
    assert_eq!(
        receive_diagnostics(&client),
        recovery_publication(&uri, 7, RECOVERY)
    );
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
            handle_single_document_notification(&server, notification, &mut documents),
            Err(NotificationError::ExpectedSingleFullChange)
        ));
        assert_document(&documents, &uri, 7, RECOVERY);
        assert!(
            client.receiver.try_recv().is_err(),
            "invalid change published"
        );
    }
    // 单文档先判 unopened，再检查 full-change shape；空 changes 在 unopened URI 上被忽略。
    let unopened: Uri = "file:///never-opened.ko".parse().expect("URI");
    let ignored = Notification::new(
        DidChangeTextDocument::METHOD.to_owned(),
        serde_json::json!({"textDocument":{"uri":unopened,"version":1}, "contentChanges":[]}),
    );
    handle_single_document_notification(&server, ignored, &mut documents)
        .expect("unopened change ignored");
    assert_document(&documents, &uri, 7, RECOVERY);
    assert!(client.receiver.try_recv().is_err());
}

// CI-only: 故意断开 transport；保留断言，禁止本地执行或扩入普通精确选集。
#[test]
fn legacy_open_and_change_publish_before_commit_but_close_removes_before_publish() {
    let uri: Uri = "file:///legacy-disconnected.ko".parse().expect("URI");
    for action in ["open", "duplicate-open", "change", "close"] {
        let (server, client) = Connection::memory();
        let mut documents = BTreeMap::new();
        if action != "open" {
            handle_single_document_notification(&server, open(&uri, 7, RECOVERY), &mut documents)
                .expect("initial commit");
            assert_eq!(
                receive_diagnostics(&client),
                recovery_publication(&uri, 7, RECOVERY)
            );
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
                handle_single_document_notification(&server, notification, &mut documents),
                Err(NotificationError::Server(ServerError::Disconnected))
            ),
            "{action}"
        );
        if matches!(action, "open" | "close") {
            assert!(
                documents.is_empty(),
                "{action}: single-document transaction boundary"
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
    assert_recovery_snapshot(&document.analysis, document.source_id, uri, source);
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
