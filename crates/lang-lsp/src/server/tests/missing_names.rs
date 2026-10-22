//! 普通无名声明走真实单文档协议；open 发布诊断、change 修正且健康导航持续可用。

use lsp_types::{DiagnosticSeverity, NumberOrString, Range, Uri};

use super::*;

const NEIGHBORS: &str = "\nfun anchor(): Int = 1\n\
    class Healthy {\nfun target(): Int = anchor()\nfun use(): Int = target()\n}";

#[test]
fn single_document_missing_names_publish_and_repair_without_losing_healthy_navigation() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize(&client);
    for (index, (broken, repaired, code, name_token, empty)) in [
        (
            "fun (): Int = true",
            "fun restored(): Int = 1",
            "L0018",
            "(",
            true,
        ),
        (
            "fun 1(): Int = true",
            "fun restored(): Int = 1",
            "L0018",
            "1",
            false,
        ),
        (
            "val : Int = true",
            "val restored: Int = 1",
            "L0018",
            ":",
            true,
        ),
        (
            "val 1: Int = true",
            "val restored: Int = 1",
            "L0018",
            "1",
            false,
        ),
        (
            "const val : Int = true",
            "const val RESTORED: Int = 1",
            "L0018",
            ":",
            true,
        ),
        (
            "const val 1: Int = true",
            "const val RESTORED: Int = 1",
            "L0018",
            "1",
            false,
        ),
        (
            "class { fun f(): Int = true }",
            "class Restored { fun f(): Int = 1 }",
            "L0067",
            "{",
            true,
        ),
        (
            "class 1 { fun f(): Int = true }",
            "class Restored { fun f(): Int = 1 }",
            "L0067",
            "1",
            false,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let uri: Uri = format!("file:///missing-name-{index}.ko")
            .parse()
            .expect("URI");
        // Unicode 与 CRLF 是普通源码，预期由源码自身的 UTF-16 长度计算。
        let broken = format!("// 界😀\r\n{broken}{NEIGHBORS}");
        send_notification(
            &client,
            DidOpenTextDocument::METHOD,
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    uri.clone(),
                    "koven".to_owned(),
                    1,
                    broken.clone(),
                ),
            })
            .expect("open params"),
        );
        let publication = receive_diagnostics(&client);
        assert_eq!(publication.uri, uri);
        assert_eq!(publication.version, Some(1));
        let start = position_of(&broken, name_token, 0);
        let end = if empty {
            start
        } else {
            Position::new(start.line, start.character + 1)
        };
        let mismatch = position_of(&broken, "true", 0);
        let expected = [
            (code, Range::new(start, end)),
            (
                "L0084",
                Range::new(
                    mismatch,
                    Position::new(mismatch.line, mismatch.character + 4),
                ),
            ),
        ];
        assert_eq!(
            publication.diagnostics.len(),
            expected.len(),
            "{publication:?}"
        );
        for (diagnostic, (code, range)) in publication.diagnostics.iter().zip(expected) {
            assert_eq!(
                diagnostic.code,
                Some(NumberOrString::String(code.to_owned()))
            );
            assert_eq!(diagnostic.range, range);
            assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
            assert_eq!(diagnostic.source.as_deref(), Some("kovenc"));
            assert!(!diagnostic.message.is_empty());
        }
        let request = 10 + i32::try_from(index).expect("small fixed case index") * 10;
        assert_healthy_navigation(&client, &uri, &broken, request);

        let repaired = format!("// 界😀\r\n{repaired}{NEIGHBORS}");
        send_notification(
            &client,
            DidChangeTextDocument::METHOD,
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: repaired.clone(),
                }],
            })
            .expect("change params"),
        );
        assert_eq!(
            receive_diagnostics(&client),
            PublishDiagnosticsParams::new(uri.clone(), Vec::new(), Some(2)),
        );
        assert_healthy_navigation(&client, &uri, &repaired, request + 2);
        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::to_value(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
            })
            .expect("close params"),
        );
        assert_eq!(
            receive_diagnostics(&client),
            PublishDiagnosticsParams::new(uri, Vec::new(), None),
        );
    }
    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

fn assert_healthy_navigation(client: &Connection, uri: &Uri, source: &str, request: i32) {
    for (offset, name) in [(0, "anchor"), (1, "target")] {
        assert_scalar_definition(
            client,
            RequestId::from(request + offset),
            uri,
            position_of(source, name, 1),
            uri,
            source,
            name,
            0,
        );
    }
}
