//! Same malformed bytes, intentionally different recovery and strict source-set gates.
use super::*;
use lsp_types::{DiagnosticSeverity, NumberOrString, Range, Uri};

const SOURCE: &str = "value class Vault(private val hidden: Int, val open: Int)\n\
fun reject(v: Vault): Int { val (open, hidden = v; return open }";

#[test]
fn missing_pattern_closer_preserves_single_recovery_and_source_set_gate() {
    let uri: Uri = "file:///workspace/broken.ko".parse().unwrap();
    for source_set in [false, true] {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        if source_set {
            initialize_with_options(
                &client,
                Some(serde_json::json!({"koven": {"sourceSet": {
                    "schema": "koven.lsp.source-set", "version": 1,
                    "roots": ["main"], "sources": [{"root": "main",
                        "logicalPath": "broken.ko", "uri": uri, "text": SOURCE}]
                }}})),
            );
        } else {
            initialize(&client);
            send_notification(
                &client,
                DidOpenTextDocument::METHOD,
                serde_json::to_value(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        uri.clone(),
                        "koven".to_owned(),
                        1,
                        SOURCE.to_owned(),
                    ),
                })
                .unwrap(),
            );
        }
        let publication = receive_diagnostics(&client);
        // Finish the real protocol before assertions, including on an oracle mismatch.
        shutdown(&client);
        server_thread.join().unwrap().unwrap();
        assert_eq!(publication.uri, uri);
        assert_eq!(publication.version, if source_set { None } else { Some(1) });
        let codes = publication
            .diagnostics
            .iter()
            .map(|d| match d.code.as_ref().unwrap() {
                NumberOrString::String(code) => code.as_str(),
                other => panic!("unexpected diagnostic code: {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            if source_set {
                vec!["L0010"]
            } else {
                vec!["L0080", "L0010"]
            }
        );
        for diagnostic in &publication.diagnostics {
            assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
            assert_eq!(diagnostic.source.as_deref(), Some("kovenc"));
            assert!(!diagnostic.message.contains("InternalError"));
            for related in diagnostic.related_information.iter().flatten() {
                assert_eq!(related.location.uri, uri);
            }
            let code = diagnostic.code.as_ref().unwrap();
            if *code == NumberOrString::String("L0010".to_owned()) {
                assert_eq!(
                    diagnostic.range,
                    Range::new(Position::new(1, 46), Position::new(1, 46))
                );
                let labels = diagnostic
                    .related_information
                    .as_ref()
                    .expect("opening delimiter label");
                assert_eq!(labels.len(), 1);
                assert_eq!(
                    labels[0].location.range,
                    Range::new(Position::new(1, 32), Position::new(1, 33))
                );
            } else {
                // The inaccessible destructuring binding, not the field declaration.
                assert_eq!(
                    diagnostic.range,
                    Range::new(Position::new(1, 39), Position::new(1, 45))
                );
            }
        }
    }
}
