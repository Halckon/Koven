use super::*;
use lsp_types::{TextEdit, Uri, request::Formatting};

fn request_formatting(client: &Connection, uri: &Uri) -> Option<Vec<TextEdit>> {
    send_request(
        client,
        RequestId::from(80),
        Formatting::METHOD,
        serde_json::json!({
            "textDocument": {"uri": uri},
            "options": {"tabSize": 4, "insertSpaces": true}
        }),
    );
    serde_json::from_value(
        receive_response(client)
            .response_result
            .expect("formatting result"),
    )
    .expect("formatting edits")
}

fn open(client: &Connection, uri: &Uri, version: i32, text: &str) {
    send_notification(
        client,
        DidOpenTextDocument::METHOD,
        serde_json::json!({
            "textDocument": {"uri": uri, "languageId": "koven", "version": version, "text": text}
        }),
    );
}

#[test]
fn formatting_uses_frontend_output_and_utf16_full_range_in_both_modes() {
    for project in [false, true] {
        let (server, client) = Connection::memory();
        let worker = thread::spawn(|| run(server));
        let uri: Uri = "file:///not-read/main.ko".parse().expect("URI");
        let options = project.then(|| {
            serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set", "version": 1, "roots": ["main"],
                "sources": [{"root": "main", "logicalPath": "main.ko", "uri": uri, "text": ""}]
            }}})
        });
        initialize_with_options(&client, options);
        if project {
            receive_diagnostics(&client);
        }
        let text = "val text=\"😀\"";
        open(&client, &uri, 3, text);
        receive_diagnostics(&client);
        let edits = request_formatting(&client, &uri).expect("edits");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start, Position::new(0, 0));
        assert_eq!(edits[0].range.end, Position::new(0, 13));
        assert_eq!(edits[0].new_text, "val text = \"😀\"");
        shutdown(&client);
        worker.join().expect("server thread").expect("server");
    }
}

#[test]
fn formatting_does_not_edit_closed_or_syntactically_invalid_documents() {
    let (server, client) = Connection::memory();
    let worker = thread::spawn(|| run(server));
    let uri: Uri = "file:///not-read/main.ko".parse().expect("URI");
    initialize(&client);
    assert_eq!(request_formatting(&client, &uri), None);
    open(&client, &uri, 1, "val item =");
    assert!(!receive_diagnostics(&client).diagnostics.is_empty());
    assert_eq!(request_formatting(&client, &uri), None);
    send_notification(
        &client,
        DidCloseTextDocument::METHOD,
        serde_json::json!({"textDocument": {"uri": uri}}),
    );
    receive_diagnostics(&client);
    assert_eq!(request_formatting(&client, &uri), None);
    shutdown(&client);
    worker.join().expect("server thread").expect("server");
}

fn initialize_mode(client: &Connection, uri: &Uri, project: bool) {
    initialize_with_options(client, project.then(|| serde_json::json!({"koven": {"sourceSet": {
        "schema": "koven.lsp.source-set", "version": 1, "roots": ["main"],
        "sources": [{"root": "main", "logicalPath": "main.ko", "uri": uri, "text": "val base=1"}]
    }}})));
    if project {
        receive_diagnostics(client);
    }
}

fn change(client: &Connection, uri: &Uri, version: i32, text: &str) {
    send_notification(
        client,
        DidChangeTextDocument::METHOD,
        serde_json::json!({
            "textDocument": {"uri": uri, "version": version},
            "contentChanges": [{"text": text}]
        }),
    );
}

#[test]
fn project_formatting_retains_existing_version_and_duplicate_open_protection() {
    {
        let project = true;
        let (server, client) = Connection::memory();
        let worker = thread::spawn(|| run(server));
        let uri: Uri = "file:///not-read/main.ko".parse().expect("URI");
        initialize_mode(&client, &uri, project);
        // Even known immutable base text is not an opened editor buffer.
        assert_eq!(request_formatting(&client, &uri), None);
        open(&client, &uri, 10, "val current=10");
        receive_diagnostics(&client);
        for version in [9, 10] {
            change(&client, &uri, version, "val stale=0");
            assert!(receive_log(&client).message.contains("not newer"));
            assert_eq!(
                request_formatting(&client, &uri).expect("edits")[0].new_text,
                "val current = 10"
            );
        }
        open(&client, &uri, 1, "val stale=1");
        assert!(receive_log(&client).message.contains("duplicated"));
        assert_eq!(
            request_formatting(&client, &uri).expect("edits")[0].new_text,
            "val current = 10"
        );
        change(&client, &uri, 11, "val current=11");
        assert_eq!(receive_diagnostics(&client).version, Some(11));
        assert_eq!(
            request_formatting(&client, &uri).expect("edits")[0].new_text,
            "val current = 11"
        );
        shutdown(&client);
        worker.join().expect("thread").expect("server");
    }
}

#[test]
fn formatting_preserves_frontend_bytes_and_does_not_mutate_the_document() {
    use lang_frontend::{formatting::format_source, source::SourceMap};
    let cases = [
        "fun f():Unit {\n// exact comment\nval text=\"a ${1+2} 😀\"\n}\n",
        "fun f():Unit {\r\n\tval text=\"a  b\"/* exact\ncomment */\r\n}\r\n",
        "val unresolved=missing", // Name errors do not make syntax unformattable.
        " \t\r\n",
        "",
    ];
    for project in [false, true] {
        let (server, client) = Connection::memory();
        let worker = thread::spawn(|| run(server));
        let uri: Uri = "file:///not-read/main.ko".parse().expect("URI");
        initialize_mode(&client, &uri, project);
        open(&client, &uri, 0, "");
        receive_diagnostics(&client);
        for (index, text) in cases.into_iter().enumerate() {
            let version = i32::try_from(index * 2 + 1).expect("version");
            change(&client, &uri, version, text);
            receive_diagnostics(&client);
            let mut sources = SourceMap::new();
            let id = sources.add_source("oracle.ko", text).expect("source");
            let expected = format_source(&sources, id).expect("valid syntax");
            let edits = request_formatting(&client, &uri).expect("valid syntax edits");
            // Repeated query must still see the original buffer: no implicit application.
            assert_eq!(request_formatting(&client, &uri), Some(edits.clone()));
            if text == expected {
                assert!(edits.is_empty());
            } else {
                assert_eq!(edits.len(), 1);
                assert_eq!(edits[0].new_text, expected);
                assert_eq!(edits[0].range.start, Position::new(0, 0));
                let line = text.bytes().filter(|byte| *byte == b'\n').count();
                let tail = text.rsplit('\n').next().expect("last line");
                assert_eq!(
                    edits[0].range.end,
                    Position::new(
                        u32::try_from(line).expect("line"),
                        u32::try_from(tail.encode_utf16().count()).expect("column")
                    )
                );
            }
            change(&client, &uri, version + 1, &expected);
            receive_diagnostics(&client);
            assert_eq!(request_formatting(&client, &uri), Some(vec![]));
        }
        shutdown(&client);
        worker.join().expect("thread").expect("server");
    }
}

#[test]
fn formatting_options_preserve_toolchain_style_and_invalid_params_remain_errors() {
    for project in [false, true] {
        let (server, client) = Connection::memory();
        let worker = thread::spawn(|| run(server));
        let uri: Uri = "file:///not-read/main.ko".parse().expect("URI");
        initialize_mode(&client, &uri, project);
        open(&client, &uri, 1, "fun f():Unit {\r\nval x=1\r\n}\r\n\r\n");
        receive_diagnostics(&client);
        let expected = request_formatting(&client, &uri);
        send_request(
            &client,
            RequestId::from(81),
            Formatting::METHOD,
            serde_json::json!({
                "textDocument": {"uri": uri}, "options": {
                    "tabSize": 2, "insertSpaces": false, "insertFinalNewline": true,
                    "trimFinalNewlines": true, "trimTrailingWhitespace": false, "custom": "ignored"
                }
            }),
        );
        let actual: Option<Vec<TextEdit>> = serde_json::from_value(
            receive_response(&client)
                .response_result
                .expect("valid options"),
        )
        .expect("edits");
        assert_eq!(actual, expected);
        for params in [
            serde_json::json!({"textDocument": {"uri": uri}}),
            serde_json::json!({"textDocument": {"uri": uri}, "options": {"tabSize": -1, "insertSpaces": true}}),
        ] {
            send_request(&client, RequestId::from(82), Formatting::METHOD, params);
            assert_eq!(
                receive_response(&client)
                    .response_result
                    .expect_err("invalid params")
                    .code,
                ErrorCode::InvalidParams as i32
            );
        }
        for (version, text) in [(2, "val broken = $"), (3, "val broken =")] {
            change(&client, &uri, version, text);
            assert!(!receive_diagnostics(&client).diagnostics.is_empty());
            assert_eq!(request_formatting(&client, &uri), None);
        }
        change(&client, &uri, 4, "val recovered=4");
        receive_diagnostics(&client);
        assert_eq!(
            request_formatting(&client, &uri).expect("edits")[0].new_text,
            "val recovered = 4"
        );
        send_notification(
            &client,
            DidCloseTextDocument::METHOD,
            serde_json::json!({"textDocument": {"uri": uri}}),
        );
        receive_diagnostics(&client);
        assert_eq!(request_formatting(&client, &uri), None);
        shutdown(&client);
        worker.join().expect("thread").expect("server");
    }
}

#[test]
fn formatting_uses_nonfirst_project_overlay_without_replacing_other_sources() {
    let (server, client) = Connection::memory();
    let worker = thread::spawn(|| run(server));
    let first: Uri = "file:///not-read/a.ko".parse().expect("URI");
    let second: Uri = "file:///not-read/z.ko".parse().expect("URI");
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set", "version": 1, "roots": ["main"],
            "sources": [
                {"root": "main", "logicalPath": "z.ko", "uri": second, "text": "val z=1"},
                {"root": "main", "logicalPath": "a.ko", "uri": first, "text": "val a=2"}
            ]
        }}})),
    );
    receive_publications(&client, 2);
    open(&client, &first, 1, "val first=3");
    receive_publications(&client, 2);
    open(&client, &second, 1, "val second=4");
    receive_publications(&client, 2);
    assert_eq!(
        request_formatting(&client, &second).expect("second edit")[0].new_text,
        "val second = 4"
    );
    assert_eq!(
        request_formatting(&client, &first).expect("first edit")[0].new_text,
        "val first = 3"
    );
    shutdown(&client);
    worker.join().expect("thread").expect("server");
}

#[test]
fn single_formatting_observes_existing_legacy_version_acceptance() {
    let (server, client) = Connection::memory();
    let worker = thread::spawn(|| run(server));
    let uri: Uri = "file:///not-read/main.ko".parse().expect("URI");
    initialize(&client);
    open(&client, &uri, 5, "val initial=5");
    receive_diagnostics(&client);
    for (version, text, expected) in [
        (5, "val equal=5", "val equal = 5"),
        (3, "val older=3", "val older = 3"),
    ] {
        change(&client, &uri, version, text);
        assert_eq!(receive_diagnostics(&client).version, Some(version));
        assert_eq!(
            request_formatting(&client, &uri).expect("edit")[0].new_text,
            expected
        );
    }
    open(&client, &uri, 2, "val duplicate=2");
    assert_eq!(receive_diagnostics(&client).version, Some(2));
    assert_eq!(
        request_formatting(&client, &uri).expect("edit")[0].new_text,
        "val duplicate = 2"
    );
    shutdown(&client);
    worker.join().expect("thread").expect("server");
}

#[test]
fn type_and_ownership_diagnostics_do_not_block_syntax_formatting() {
    use lang_frontend::{formatting::format_source, source::SourceMap};
    for project in [false, true] {
        let (server, client) = Connection::memory();
        let worker = thread::spawn(|| run(server));
        let uri: Uri = "file:///not-read/main.ko".parse().expect("URI");
        initialize_mode(&client, &uri, project);
        open(&client, &uri, 0, "");
        receive_diagnostics(&client);
        for (version, text) in [
            (1, "fun wrong():Int = true"),
            (
                2,
                "class Resource()\nfun take(own x:Resource):Unit {}\nfun bad(own x:Resource):Unit {\ntake(x)\ntake(x)\n}",
            ),
        ] {
            change(&client, &uri, version, text);
            let publication = receive_diagnostics(&client);
            assert!(
                !publication.diagnostics.is_empty(),
                "expected semantic diagnostic"
            );
            if version == 2 {
                assert!(
                    publication
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code
                            == Some(lsp_types::NumberOrString::String("L0131".to_owned()))),
                    "expected ownership diagnostic: {publication:?}"
                );
            }
            let mut sources = SourceMap::new();
            let id = sources.add_source("oracle.ko", text).expect("source");
            let expected = format_source(&sources, id).expect("valid syntax");
            assert_eq!(
                request_formatting(&client, &uri).expect("edit")[0].new_text,
                expected
            );
        }
        shutdown(&client);
        worker.join().expect("thread").expect("server");
    }
}
