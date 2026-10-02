use lsp_types::{Location, Range, Uri};

use super::*;
use crate::{
    server::{NotificationError, ServerError, handle_unit_notification},
    source_set::SourceSetConfig,
    unit_session::{UnitSession, UnitSessionError},
};

const PROVIDER_URI: &str = "file:///workspace/p/api.ko";
const CONSUMER_URI: &str = "file:///workspace/q/use.ko";
const BASE_PROVIDER: &str = "package p\nfun make(): Int = 1";
const CONSUMER: &str = "package q\r\nimport p.make as Make\r\n\
    fun use(): Int = /* 界é😀 */ Make()\r\n";

#[test]
fn source_set_provider_overlay_moves_definition_with_unicode_crlf_and_close_restores_base() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let provider_uri: Uri = PROVIDER_URI.parse().expect("provider URI");
    let consumer_uri: Uri = CONSUMER_URI.parse().expect("consumer URI");
    initialize_with_options(&client, Some(source_set_options()));

    let base_publications = expected_publications(&provider_uri, &consumer_uri, None);
    assert_eq!(receive_publications(&client, 2), base_publications);
    assert_consumer_definitions(&client, &consumer_uri, location(&provider_uri, 1, 4), 100);

    let opened = "package p\r\n\r\n/* 界é😀 */ public fun make(): Int = 2\r\n";
    let open = open_notification(&provider_uri, 5, opened);
    send_notification(&client, DidOpenTextDocument::METHOD, open.params);
    assert_eq!(
        receive_publications(&client, 2),
        expected_publications(&provider_uri, &consumer_uri, Some(5))
    );
    assert_consumer_definitions(&client, &consumer_uri, location(&provider_uri, 2, 22), 110);

    let changed = "package p\r\n\r\n\r\n/* 界é😀 */ fun make(): Int = 3\r\n";
    let change = change_notification(&provider_uri, 6, changed);
    send_notification(&client, DidChangeTextDocument::METHOD, change.params);
    assert_eq!(
        receive_publications(&client, 2),
        expected_publications(&provider_uri, &consumer_uri, Some(6))
    );
    assert_consumer_definitions(&client, &consumer_uri, location(&provider_uri, 3, 15), 120);

    send_notification(
        &client,
        DidCloseTextDocument::METHOD,
        serde_json::to_value(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(provider_uri.clone()),
        })
        .expect("close params"),
    );
    assert_eq!(receive_publications(&client, 2), base_publications);
    assert_consumer_definitions(&client, &consumer_uri, location(&provider_uri, 1, 4), 130);

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn source_set_equal_and_older_changes_preserve_navigation_before_newer_commit() {
    let (server, client) = Connection::memory();
    let mut session = unit_session();
    let provider_uri: Uri = PROVIDER_URI.parse().expect("provider URI");
    let consumer_uri: Uri = CONSUMER_URI.parse().expect("consumer URI");
    let stable = "package p\r\n/* 界é😀 */ fun make(): Int = 5\r\n";
    handle_unit_notification(
        &server,
        open_notification(&provider_uri, 5, stable),
        &mut session,
    )
    .expect("commit version 5");
    let committed = expected_publications(&provider_uri, &consumer_uri, Some(5));
    assert_eq!(receive_publications(&client, 2), committed);
    assert_eq!(session_publications(&session), committed);
    assert_session_definitions(&session, &consumer_uri, location(&provider_uri, 1, 15));

    for version in [5, 4] {
        let rejected = handle_unit_notification(
            &server,
            change_notification(
                &provider_uri,
                version,
                "package p\r\nfun replaced(): Int = 99\r\n",
            ),
            &mut session,
        );
        assert!(matches!(
            rejected,
            Err(NotificationError::Unit(UnitSessionError::Protocol(message)))
                if message.contains("is not newer than 5")
        ));
        assert_eq!(session_publications(&session), committed);
        assert_session_definitions(&session, &consumer_uri, location(&provider_uri, 1, 15));
        assert!(
            client.receiver.try_recv().is_err(),
            "rejected update published"
        );
    }

    handle_unit_notification(
        &server,
        change_notification(
            &provider_uri,
            6,
            "package p\r\n\r\n/* 界é😀 */ fun make(): Int = 6\r\n",
        ),
        &mut session,
    )
    .expect("commit version 6 after rejected updates");
    let advanced = expected_publications(&provider_uri, &consumer_uri, Some(6));
    assert_eq!(receive_publications(&client, 2), advanced);
    assert_eq!(session_publications(&session), advanced);
    assert_session_definitions(&session, &consumer_uri, location(&provider_uri, 2, 15));
}

#[test]
fn source_set_publication_send_failure_preserves_committed_navigation_and_version() {
    let (server, client) = Connection::memory();
    let mut session = unit_session();
    let provider_uri: Uri = PROVIDER_URI.parse().expect("provider URI");
    let consumer_uri: Uri = CONSUMER_URI.parse().expect("consumer URI");
    handle_unit_notification(
        &server,
        open_notification(&provider_uri, 5, BASE_PROVIDER),
        &mut session,
    )
    .expect("commit version 5");
    let committed = expected_publications(&provider_uri, &consumer_uri, Some(5));
    assert_eq!(receive_publications(&client, 2), committed);
    assert_eq!(session_publications(&session), committed);
    assert_session_definitions(&session, &consumer_uri, location(&provider_uri, 1, 4));

    drop(client.receiver);
    let changed = "package p\r\n\r\n/* 界é😀 */ fun make(): Int = 6\r\n";
    let failed = handle_unit_notification(
        &server,
        change_notification(&provider_uri, 6, changed),
        &mut session,
    );
    assert!(matches!(
        failed,
        Err(NotificationError::Server(ServerError::Disconnected))
    ));
    assert_eq!(session_publications(&session), committed);
    assert_session_definitions(&session, &consumer_uri, location(&provider_uri, 1, 4));

    // 发送失败不能消耗版本；同一 v6 可通过真实 notification 路径重新发布并提交。
    let (reconnected_server, reconnected_client) = Connection::memory();
    handle_unit_notification(
        &reconnected_server,
        change_notification(&provider_uri, 6, changed),
        &mut session,
    )
    .expect("retry uncommitted version 6");
    let advanced = expected_publications(&provider_uri, &consumer_uri, Some(6));
    assert_eq!(receive_publications(&reconnected_client, 2), advanced);
    assert_eq!(session_publications(&session), advanced);
    assert_session_definitions(&session, &consumer_uri, location(&provider_uri, 2, 15));
}

#[test]
fn source_set_definition_returns_null_for_out_of_range_utf16_positions() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let provider_uri: Uri = PROVIDER_URI.parse().expect("provider URI");
    let consumer_uri: Uri = CONSUMER_URI.parse().expect("consumer URI");
    initialize_with_options(&client, Some(source_set_options()));
    assert_eq!(
        receive_publications(&client, 2),
        expected_publications(&provider_uri, &consumer_uri, None)
    );

    let call_line_width = u32::try_from(
        CONSUMER
            .lines()
            .nth(2)
            .expect("consumer call line")
            .encode_utf16()
            .count(),
    )
    .expect("UTF-16 line width");
    for (index, position) in [
        Position::new(0, 10), // package q 的 CRLF 不属于行内容。
        Position::new(2, call_line_width + 1),
        Position::new(4, 0),
        Position::new(u32::MAX, 0),
        Position::new(2, u32::MAX),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            request_definition(
                &client,
                RequestId::from(140 + i32::try_from(index).expect("request index")),
                &consumer_uri,
                position,
            ),
            None,
            "out-of-range position {position:?}"
        );
    }
    assert_consumer_definitions(&client, &consumer_uri, location(&provider_uri, 1, 4), 150);

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

fn source_set_options() -> serde_json::Value {
    serde_json::json!({"koven": {"sourceSet": {
        "schema": "koven.lsp.source-set",
        "version": 1,
        "roots": ["main"],
        "sources": [
            {
                "root": "main",
                "logicalPath": "q/use.ko",
                "uri": CONSUMER_URI,
                "text": CONSUMER
            },
            {
                "root": "main",
                "logicalPath": "p/api.ko",
                "uri": PROVIDER_URI,
                "text": BASE_PROVIDER
            }
        ]
    }}})
}

fn unit_session() -> UnitSession {
    let options = source_set_options();
    let config = SourceSetConfig::from_initialization_options(Some(&options))
        .expect("source-set options")
        .expect("unit mode");
    UnitSession::new(config).expect("initial session")
}

fn open_notification(uri: &Uri, version: i32, text: &str) -> Notification {
    Notification {
        method: DidOpenTextDocument::METHOD.to_owned(),
        params: serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri.clone(),
                "koven".to_owned(),
                version,
                text.to_owned(),
            ),
        })
        .expect("open params"),
    }
}

fn change_notification(uri: &Uri, version: i32, text: &str) -> Notification {
    Notification {
        method: DidChangeTextDocument::METHOD.to_owned(),
        params: serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_owned(),
            }],
        })
        .expect("change params"),
    }
}

fn expected_publications(
    provider_uri: &Uri,
    consumer_uri: &Uri,
    provider_version: Option<i32>,
) -> Vec<PublishDiagnosticsParams> {
    vec![
        PublishDiagnosticsParams::new(provider_uri.clone(), Vec::new(), provider_version),
        PublishDiagnosticsParams::new(consumer_uri.clone(), Vec::new(), None),
    ]
}

fn session_publications(session: &UnitSession) -> Vec<PublishDiagnosticsParams> {
    session
        .publications()
        .expect("committed publications")
        .into_iter()
        .map(|publication| {
            PublishDiagnosticsParams::new(
                publication.uri,
                publication.diagnostics,
                publication.version,
            )
        })
        .collect()
}

fn location(uri: &Uri, line: u32, character: u32) -> Location {
    Location::new(
        uri.clone(),
        Range::new(
            Position::new(line, character),
            Position::new(line, character + 4),
        ),
    )
}

fn assert_consumer_definitions(
    client: &Connection,
    consumer_uri: &Uri,
    expected: Location,
    first_request: i32,
) {
    for (index, (needle, occurrence)) in [("make", 0), ("Make", 0), ("Make", 1)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            request_definition(
                client,
                RequestId::from(first_request + i32::try_from(index).expect("request index")),
                consumer_uri,
                position_of(CONSUMER, needle, occurrence),
            ),
            Some(GotoDefinitionResponse::Scalar(expected.clone()))
        );
    }
}

fn assert_session_definitions(session: &UnitSession, consumer_uri: &Uri, expected: Location) {
    for (needle, occurrence) in [("make", 0), ("Make", 0), ("Make", 1)] {
        assert_eq!(
            session
                .definition_locations(consumer_uri, position_of(CONSUMER, needle, occurrence))
                .expect("committed definition"),
            vec![expected.clone()]
        );
    }
}
